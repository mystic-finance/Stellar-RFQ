import { rpc } from "@stellar/stellar-sdk";
import type { AssembledTransaction } from "@stellar/stellar-sdk/contract";

export interface Submitted<T> {
  hash: string;
  ledger: number;
  /** The contract's return value; undefined for functions that return nothing. */
  result: T | undefined;
}

/**
 * Signs, sends and confirms a simulated call (throwing if it failed on-chain),
 * then waits until the RPC's
 * latest ledger includes it, so a read straight after sees the write.
 * Use it instead of `signAndSend()` when you need the hash or the next read
 * to be consistent; it also tolerates functions that return nothing.
 */
export async function submit<T>(tx: AssembledTransaction<T> | Promise<AssembledTransaction<T>>): Promise<Submitted<T>> {
  const assembled = await tx;
  const sent = await assembled.signAndSend();
  const res = sent.getTransactionResponse as rpc.Api.GetSuccessfulTransactionResponse | undefined;
  const hash = sent.sendTransactionResponse?.hash ?? "";
  // signAndSend can resolve on a transaction that failed on-chain (for example,
  // state moved between simulation and execution); surface it.
  if (res?.status !== rpc.Api.GetTransactionStatus.SUCCESS) {
    throw new Error(`transaction ${hash} did not succeed: ${res?.status ?? "no response"}`);
  }
  const ledger = res.ledger;
  let result: T | undefined;
  try {
    result = sent.result;
  } catch {
    result = undefined; // a void return the SDK cannot parse
  }
  const o = assembled.options;
  const server = new rpc.Server(o.rpcUrl, { allowHttp: o.allowHttp });
  while ((await server.getLatestLedger()).sequence < ledger) await new Promise((r) => setTimeout(r, 1000));
  return { hash, ledger, result };
}
