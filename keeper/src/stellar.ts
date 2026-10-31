import { Address, Contract, Keypair, nativeToScVal, rpc, scValToNative, TransactionBuilder, xdr } from "@stellar/stellar-sdk";

/** The push oracle, as the keeper uses it. */
export class PushOracle {
  private server: rpc.Server;
  private contract: Contract;

  constructor(private opts: { rpcUrl: string; passphrase: string; oracle: string; signer: Keypair }) {
    this.server = new rpc.Server(opts.rpcUrl, { allowHttp: opts.rpcUrl.startsWith("http://") });
    this.contract = new Contract(opts.oracle);
  }

  /** The last pushed price, or null before the first push (`NoPrice`, contract error 5). */
  async lastPrice(base: string, quote: string): Promise<{ price: bigint; timestamp: number } | null> {
    try {
      const v = await this.simulate("get_price", [new Address(base).toScVal(), new Address(quote).toScVal()]);
      const p = scValToNative(v) as { price: bigint; timestamp: bigint };
      return { price: BigInt(p.price), timestamp: Number(p.timestamp) };
    } catch (e) {
      if (String(e).includes("Error(Contract, #5)")) return null;
      throw e;
    }
  }

  async maxDeviationBps(): Promise<number> {
    return Number(scValToNative(await this.simulate("max_deviation_bps", [])));
  }

  async push(base: string, quote: string, price: bigint, timestamp: number): Promise<string> {
    const op = this.contract.call(
      "push",
      new Address(this.opts.signer.publicKey()).toScVal(),
      new Address(base).toScVal(),
      new Address(quote).toScVal(),
      nativeToScVal(price, { type: "i128" }),
      nativeToScVal(timestamp, { type: "u64" }),
    );
    const account = await this.server.getAccount(this.opts.signer.publicKey());
    const tx = new TransactionBuilder(account, { fee: "1000000", networkPassphrase: this.opts.passphrase })
      .addOperation(op)
      .setTimeout(60)
      .build();
    const prepared = await this.server.prepareTransaction(tx);
    prepared.sign(this.opts.signer);
    const sent = await this.server.sendTransaction(prepared);
    if (sent.status === "ERROR") throw new Error(`push rejected: ${JSON.stringify(sent.errorResult)}`);
    for (let i = 0; i < 30; i++) {
      const got = await this.server.getTransaction(sent.hash);
      if (got.status === "SUCCESS") return sent.hash;
      if (got.status === "FAILED") throw new Error(`push failed: ${sent.hash}`);
      await new Promise((r) => setTimeout(r, 2000));
    }
    throw new Error(`push not confirmed: ${sent.hash}`);
  }

  private async simulate(method: string, args: xdr.ScVal[]): Promise<xdr.ScVal> {
    const account = await this.server.getAccount(this.opts.signer.publicKey());
    const tx = new TransactionBuilder(account, { fee: "100", networkPassphrase: this.opts.passphrase })
      .addOperation(this.contract.call(method, ...args))
      .setTimeout(30)
      .build();
    const sim = await this.server.simulateTransaction(tx);
    if (rpc.Api.isSimulationError(sim)) throw new Error(`${method}: ${sim.error}`);
    return sim.result!.retval;
  }
}
