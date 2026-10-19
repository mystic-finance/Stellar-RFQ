import { Address, Keypair, xdr } from "@stellar/stellar-sdk";
import { basicNodeSigner, type ClientOptions } from "@stellar/stellar-sdk/contract";
import * as rfq from "./contracts/rfq.js";
import * as router from "./contracts/router.js";
import * as facility from "./contracts/facility.js";
import * as vault from "./contracts/vault.js";
import * as queue from "./contracts/withdrawal_queue.js";
import * as facilityAggregator from "./contracts/facility_aggregator.js";
import * as dexAggregator from "./contracts/dex_aggregator.js";
import * as pushOracle from "./contracts/push_oracle.js";
import * as token from "./contracts/token.js";
import * as venue from "./contracts/venue.js";
import { networks, type Deployment, type NetworkName } from "./networks.js";

export { rfq, router, facility, vault, queue, facilityAggregator, dexAggregator, pushOracle, token, venue };
export * from "./networks.js";
export * from "./encode.js";
export * from "./pricing.js";
export * from "./orders.js";
export * from "./submit.js";
export * from "./auction.js";

/** Who signs: a wallet in the browser (Freighter, Stellar Wallets Kit), a keypair on a server. */
export interface Signer {
  publicKey: string;
  /** Wallet-kit shape: XDR in, signed XDR out. */
  signTransaction: (
    xdr: string,
    opts?: { networkPassphrase?: string; address?: string },
  ) => Promise<{ signedTxXdr: string; signerAddress?: string }>;
  signAuthEntry?: ClientOptions["signAuthEntry"];
}

/** A server-side signer from a secret key. */
export function keypairSigner(secret: string | Keypair, networkPassphrase: string): Signer {
  const kp = typeof secret === "string" ? Keypair.fromSecret(secret) : secret;
  const s = basicNodeSigner(kp, networkPassphrase);
  return { publicKey: kp.publicKey(), signTransaction: s.signTransaction as Signer["signTransaction"], signAuthEntry: s.signAuthEntry };
}

export interface OctarineOptions {
  network: NetworkName | { rpcUrl: string; networkPassphrase: string };
  /** Deployed addresses; defaults to the published deployment for the network. */
  contracts?: Partial<Deployment>;
  /** Omit for read-only use: views simulate without a signature. */
  signer?: Signer;
  allowHttp?: boolean;
}

/**
 * Entry point. Every method returns a typed contract client; calls return an
 * `AssembledTransaction` that is already simulated: read `.result` for views,
 * call `.signAndSend()` for writes.
 */
export class Octarine {
  readonly rpcUrl: string;
  readonly networkPassphrase: string;
  readonly contracts: Partial<Deployment>;
  readonly signer?: Signer;
  private allowHttp: boolean;

  constructor(opts: OctarineOptions) {
    const net = typeof opts.network === "string" ? networks[opts.network] : opts.network;
    this.rpcUrl = net.rpcUrl;
    this.networkPassphrase = net.networkPassphrase;
    this.contracts = { ...(typeof opts.network === "string" ? networks[opts.network].contracts : {}), ...opts.contracts };
    this.signer = opts.signer;
    this.allowHttp = opts.allowHttp ?? this.rpcUrl.startsWith("http://");
  }

  /** The same SDK acting as someone else (e.g. the LP, then the trader). */
  as(signer: Signer): Octarine {
    return new Octarine({
      network: { rpcUrl: this.rpcUrl, networkPassphrase: this.networkPassphrase },
      contracts: this.contracts,
      signer,
      allowHttp: this.allowHttp,
    });
  }

  options(contractId: string): ClientOptions {
    return {
      contractId,
      rpcUrl: this.rpcUrl,
      networkPassphrase: this.networkPassphrase,
      allowHttp: this.allowHttp,
      publicKey: this.signer?.publicKey,
      signTransaction: this.signer?.signTransaction as ClientOptions["signTransaction"],
      signAuthEntry: this.signer?.signAuthEntry,
    };
  }

  rfq(id = this.need("rfq")) {
    return new rfq.Client(this.options(id));
  }
  router(id = this.need("router")) {
    return new router.Client(this.options(id));
  }
  facilityAggregator(id = this.need("facilityAggregator")) {
    return new facilityAggregator.Client(this.options(id));
  }
  dexAggregator(id = this.need("dexAggregator")) {
    return new dexAggregator.Client(this.options(id));
  }
  facility(id: string) {
    return new facility.Client(this.options(id));
  }
  vault(id: string) {
    return new vault.Client(this.options(id));
  }
  queue(id: string) {
    return new queue.Client(this.options(id));
  }
  pushOracle(id = this.need("pushOracle")) {
    return new pushOracle.Client(this.options(id));
  }
  /** A vault's venue adapter (Blend, DeFindex, SEP-56): `total_assets`, `max_withdraw`. */
  venue(id: string) {
    return new venue.Client(this.options(id));
  }
  /** Any SEP-41 token, including Stellar Asset Contracts. */
  token(id: string) {
    return new token.Client(this.options(id));
  }

  /** Every facility the aggregator routes to, with its oLP NAV. */
  async facilities(): Promise<{ id: string; totalAssets: bigint; totalSupply: bigint; base: string }[]> {
    const ids = (await this.facilityAggregator().active()).result;
    return Promise.all(
      ids.map(async (id) => {
        const f = this.facility(id);
        const [a, s, b] = await Promise.all([f.total_assets(), f.total_supply(), f.base()]);
        return { id, totalAssets: a.result, totalSupply: s.result, base: b.result };
      }),
    );
  }

  private need(key: keyof Deployment): string {
    const id = this.contracts[key];
    if (!id) throw new Error(`no ${key} address for this network; pass it in options.contracts`);
    return id;
  }
}

/** A `G…`/`C…` string as the ScVal contracts receive. */
export const addressVal = (a: string): xdr.ScVal => new Address(a).toScVal();
