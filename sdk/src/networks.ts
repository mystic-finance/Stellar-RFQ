import { deployments } from "./deployments.js";

export type NetworkName = "testnet" | "mainnet";

/** Addresses the SDK needs; anything else (facilities, vaults) is discovered on-chain. */
export interface Deployment {
  rfq: string;
  router: string;
  facilityAggregator: string;
  dexAggregator: string;
  pushOracle: string;
  soroswapAdapter: string;
  aquariusAdapter: string;
}

export interface Network {
  rpcUrl: string;
  networkPassphrase: string;
  contracts: Partial<Deployment>;
  /** Code the facility aggregator deploys, and the venue adapter code it allowlists. */
  wasmHashes: Partial<Record<"vault" | "facility" | "queue" | "blendAdapter" | "defindexAdapter" | "sep56Adapter" | "mockVenue", string>>;
  /** Assets and third-party protocols this deployment was verified against. */
  external: Partial<Record<"xlm" | "usdc" | "blendPoolV2" | "defindexUsdcVault" | "soroswapRouter" | "aquariusXlmUsdcPool", string>> & Record<string, string>;
}

export const networks: Record<NetworkName, Network> = {
  testnet: {
    rpcUrl: "https://soroban-testnet.stellar.org",
    networkPassphrase: "Test SDF Network ; September 2015",
    ...deployments.testnet,
  },
  mainnet: {
    rpcUrl: "https://mainnet.sorobanrpc.com",
    networkPassphrase: "Public Global Stellar Network ; September 2015",
    ...deployments.mainnet,
  },
};
