import { Keypair } from "@stellar/stellar-sdk";

const need = (k: string): string => {
  const v = process.env[k];
  if (!v) throw new Error(`missing env ${k}`);
  return v;
};

/** Everything comes from the environment; the signing key never touches a file in the repo. */
export function load() {
  return {
    rpcUrl: process.env.STELLAR_RPC_URL ?? "https://soroban-testnet.stellar.org",
    passphrase: process.env.STELLAR_PASSPHRASE ?? "Test SDF Network ; September 2015",
    oracle: need("PUSH_ORACLE"),
    signer: Keypair.fromSecret(need("KEEPER_SECRET")),
    /** Stellar asset contracts priced: base in quote. */
    base: need("NAV_BASE"),
    quote: need("NAV_QUOTE"),
    baseDecimals: Number(process.env.NAV_BASE_DECIMALS ?? 18),
    quoteDecimals: Number(process.env.NAV_QUOTE_DECIMALS ?? 7),
    /** Default source: Centrifuge's JAAA/USDC vault on Ethereum. */
    evmRpcUrl: process.env.EVM_RPC_URL ?? "https://ethereum-rpc.publicnode.com",
    vault: process.env.NAV_VAULT ?? "0x4880799ee5200fc58da299e965df644fbf46780b",
    intervalSeconds: Number(process.env.INTERVAL_SECONDS ?? 300),
    minChangeBps: Number(process.env.MIN_CHANGE_BPS ?? 5),
    heartbeatSeconds: Number(process.env.HEARTBEAT_SECONDS ?? 6 * 3600),
  };
}
