import { Keypair, Operation, rpc, TransactionBuilder, xdr } from "@stellar/stellar-sdk";
import { Buffer } from "buffer";
import { signOrderHash } from "./orders.js";
import type { Octarine } from "./index.js";
import type { RfqOrder } from "./contracts/rfq.js";

/** Octarine's Stellar chain ids in the auction API. */
export const STELLAR_CHAIN_IDS = { mainnet: 100100, testnet: 100101 } as const;

export interface AuctionOptions {
  /** e.g. https://curator-api.mysticfinance.xyz */
  baseUrl: string;
  /** Market-maker API key; sent as `x-api-key`. Not needed for public reads. */
  apiKey?: string;
  chainId?: number;
  fetch?: typeof fetch;
}

/** An RFQ request as the API publishes it (the fields bidders use). */
export interface AuctionRequest {
  requestId: string;
  walletAddress: string;
  /** Token the taker sells (`taker_token`). */
  redeemAsset: string;
  /** Token the taker receives (`maker_token`). */
  redemptionAsset: string;
  redeemAmount: string;
  chainId: number;
  status: string;
  expiryTime: number;
  biddingCloseTime: number;
  bidCount?: number;
  bestBidAmount?: string;
  metadata: {
    requestHash?: string;
    horizonSeconds?: number;
    minHorizonSeconds?: number;
    /** Highest rate a bid may carry, in bps/day (human units). */
    maxBpsPerDay?: number;
    indicativeMakerAmount?: string;
    rfqContract?: string;
    routerContract?: string;
    redeemAssetData?: { address: string; symbol: string; decimals: number };
    redemptionAssetData?: { address: string; symbol: string; decimals: number };
    [k: string]: unknown;
  };
  [k: string]: unknown;
}

export interface BidQuote {
  requestId: string;
  /** The exact order to sign, as the API serialises it (numbers as strings). */
  order: Record<string, unknown>;
  orderHash: string;
  makerBpsPerDay: number;
  maxMakerAmount: string;
  indicativeMakerAmount: string;
  horizonSeconds: number;
  maxBpsPerDay: number;
}

export interface Page<T> {
  data: T[];
  page: number;
  limit: number;
  totalPages: number;
  totalItems: number;
}

/** Signature in the API's packed form: r = ed25519 public key, s = signature. */
export const packSignature = (s: { signer: Uint8Array; signature: Uint8Array }) => ({
  signatureType: 7,
  v: 0,
  r: Buffer.from(s.signer).toString("hex"),
  s: Buffer.from(s.signature).toString("hex"),
});

/**
 * The Octarine auction API (mystic-backend), Stellar flavour. Market makers
 * read requests, quote and submit signed bids; takers create requests and
 * fetch the transactions that fill a chosen bid.
 */
export class AuctionClient {
  readonly chainId: number;
  private f: typeof fetch;

  constructor(private opts: AuctionOptions) {
    this.chainId = opts.chainId ?? STELLAR_CHAIN_IDS.testnet;
    this.f = opts.fetch ?? fetch;
  }

  private async call<T>(method: string, path: string, body?: unknown): Promise<T> {
    const res = await this.f(`${this.opts.baseUrl.replace(/\/$/, "")}${path}`, {
      method,
      headers: {
        "content-type": "application/json",
        ...(this.opts.apiKey ? { "x-api-key": this.opts.apiKey } : {}),
      },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const text = await res.text();
    const json = text ? JSON.parse(text) : undefined;
    if (!res.ok) throw new AuctionError(res.status, json?.message ?? text, json);
    return json as T;
  }

  // ---- reads (public)

  /** Open requests on this chain; default status is pending + bidding. */
  requests(q: { status?: string; marketMaker?: string; excludeBidOn?: boolean; page?: number; limit?: number } = {}) {
    const params = new URLSearchParams({ chainId: String(this.chainId) });
    for (const [k, v] of Object.entries(q)) if (v !== undefined) params.set(k, String(v));
    return this.call<Page<AuctionRequest>>("GET", `/octarine/requests?${params}`);
  }
  request(requestId: string) {
    return this.call<AuctionRequest>("GET", `/octarine/request/${requestId}`);
  }
  /** Pending bids, best first, each with the `txns` that fill it. */
  swap(requestId: string) {
    return this.call<{ status: string; bids: any[]; bestBid: any; signatureRequired: boolean; requestHash: string }>(
      "GET",
      `/octarine/swap/${requestId}`,
    );
  }
  makerBids(maker: string, q: { status?: string; page?: number; limit?: number } = {}) {
    const params = new URLSearchParams({ chainId: String(this.chainId) });
    for (const [k, v] of Object.entries(q)) if (v !== undefined) params.set(k, String(v));
    return this.call<Page<any>>("GET", `/octarine/market-maker/${maker}/bids?${params}`);
  }

  // ---- market makers (API key)

  /** The exact order a rate becomes. `makerBpsPerDay` is human units, e.g. 2.25. */
  async quoteBid(b: { requestId: string; makerBpsPerDay: number; maker?: string; maxMakerAmount?: bigint }): Promise<BidQuote> {
    const r = await this.call<{ data: BidQuote }>("POST", "/octarine/bid/quote", {
      ...b,
      maxMakerAmount: b.maxMakerAmount?.toString(),
    });
    return r.data;
  }

  submitBid(b: {
    requestId: string;
    maker: string;
    makerBpsPerDay: number;
    maxMakerAmount: bigint | string;
    signature: { signer: Uint8Array; signature: Uint8Array };
    /** Off-chain bid lifetime in minutes (default 10). */
    expiry?: number;
  }) {
    return this.call<{ success: boolean; data: any; message: string }>("POST", "/octarine/bid", {
      requestId: b.requestId,
      maker: b.maker,
      makerBpsPerDay: b.makerBpsPerDay,
      maxMakerAmount: b.maxMakerAmount.toString(),
      signature: packSignature(b.signature),
      expiry: b.expiry,
    });
  }

  cancelBid(bidId: string, maker: string) {
    return this.call<unknown>("DELETE", `/octarine/bid/${bidId}?marketMaker=${encodeURIComponent(maker)}`);
  }

  // ---- takers

  createSwap(s: { walletAddress: string; sell: string; buy: string; amount: bigint; slippageTolerance?: number; expiryMinutes?: number }) {
    return this.call<{ requestId: string; requestHash: string; signatureRequired: boolean; request: AuctionRequest }>("POST", "/octarine/swap", {
      walletAddress: s.walletAddress,
      redeemAsset: s.sell,
      redemptionAsset: s.buy,
      amount: s.amount.toString(),
      chainId: this.chainId,
      slippageTolerance: s.slippageTolerance,
      expiry: s.expiryMinutes,
    });
  }

  submitTakerSignature(requestId: string, signature: { signer: Uint8Array; signature: Uint8Array }) {
    return this.call<unknown>("POST", "/octarine/swap/signature", { requestId, signature: packSignature(signature) });
  }

  recordFill(f: { requestId: string; bidId: string; txHash: string; filledAmount: string; marketMaker: string }) {
    return this.call<unknown>("POST", "/octarine/fill", f);
  }

  /**
   * Live requests and bid updates over socket.io (`/octarine` namespace).
   * Needs `socket.io-client` installed; returns a function that disconnects.
   */
  async subscribe(handlers: {
    onRequest?: (r: AuctionRequest) => void;
    onBidStatus?: (e: { bidId: string; requestId: string; oldStatus: string; newStatus: string; data: any }) => void;
    onRequestStatus?: (e: { requestId: string; oldStatus: string; newStatus: string; data: any }) => void;
  }): Promise<() => void> {
    // Optional dependency, loaded by name so bundlers do not require it.
    const mod = "socket.io-client";
    const { io } = (await import(/* @vite-ignore */ mod)) as typeof import("socket.io-client");
    const socket = io(`${this.opts.baseUrl.replace(/\/$/, "")}/octarine`, {
      transports: ["websocket"],
      auth: this.opts.apiKey ? { apiKey: this.opts.apiKey } : undefined,
    });
    socket.on("connect", () => socket.emit("subscribe_rfqs", { chainIds: [this.chainId] }));
    socket.on("new_rfq_request", (e: { data: AuctionRequest }) => handlers.onRequest?.(e.data));
    socket.on("bid_status_update", (e: any) => handlers.onBidStatus?.(e));
    socket.on("rfq_status_update", (e: any) => handlers.onRequestStatus?.(e));
    return () => socket.disconnect();
  }
}

export class AuctionError extends Error {
  constructor(readonly status: number, message: string, readonly body?: unknown) {
    super(`auction API ${status}: ${message}`);
  }
}

/** The API's order JSON (strings) → the contract's `RfqOrder`. */
export function toRfqOrder(o: Record<string, unknown>): RfqOrder {
  const s = (k: string) => String(o[k]);
  const opt = (k: string) => (o[k] === null || o[k] === undefined || o[k] === "" ? undefined : s(k));
  return {
    expiry: BigInt(s("expiry")),
    fee_bps: Number(o.fee_bps),
    fee_recipient: s("fee_recipient"),
    maker: s("maker"),
    maker_bps_per_day: Number(o.maker_bps_per_day),
    maker_token: s("maker_token"),
    max_maker_amount: BigInt(s("max_maker_amount")),
    min_received_amount: BigInt(s("min_received_amount")),
    salt: BigInt(s("salt")),
    sender: opt("sender"),
    taker: opt("taker"),
    taker_amount: BigInt(s("taker_amount")),
    taker_max_bps_per_day: Number(o.taker_max_bps_per_day),
    taker_token: s("taker_token"),
  };
}

/**
 * The whole maker flow for one request: quote the rate, check the order and
 * its hash against the settlement contract, sign, submit. Never signs a hash
 * the contract did not produce for an order the maker would accept.
 */
export async function placeBid(
  auction: AuctionClient,
  sdk: Octarine,
  signer: Keypair,
  bid: { requestId: string; maker: string; makerBpsPerDay: number; maxMakerAmount?: bigint },
) {
  const q = await auction.quoteBid(bid);
  const order = toRfqOrder(q.order);
  if (order.maker !== bid.maker) throw new Error(`quoted order names maker ${order.maker}`);
  if (order.maker_bps_per_day !== Math.round(bid.makerBpsPerDay * 100)) throw new Error("quoted rate differs from the bid");
  if (bid.maxMakerAmount !== undefined && order.max_maker_amount > bid.maxMakerAmount) throw new Error("quoted ceiling above the bid's");
  const rfqId = (q.order.contractId as string) || (q.order.verifyingContract as string) || undefined;
  const onChain = (await sdk.rfq(rfqId).hash_rfq_order({ order })).result;
  if (Buffer.from(onChain).toString("hex") !== q.orderHash.replace(/^0x/, "")) throw new Error("order hash does not match the contract's");
  const signature = signOrderHash(signer, Buffer.from(onChain));
  return auction.submitBid({ ...bid, maxMakerAmount: order.max_maker_amount, signature });
}

/**
 * Runs the backend-built transactions for a bid (`swap(requestId).bids[i].txns`):
 * each `data` is a base64 `HostFunction`; they are simulated, signed and sent in order.
 */
export async function executeTxns(
  sdk: Octarine,
  txns: { type?: string; to: string; data: string }[],
): Promise<string[]> {
  if (!sdk.signer) throw new Error("executeTxns needs a signer");
  const server = new rpc.Server(sdk.rpcUrl);
  const hashes: string[] = [];
  for (const t of txns) {
    const func = xdr.HostFunction.fromXDR(t.data, "base64");
    const account = await server.getAccount(sdk.signer.publicKey);
    const built = new TransactionBuilder(account, { fee: "1000000", networkPassphrase: sdk.networkPassphrase })
      .addOperation(Operation.invokeHostFunction({ func, auth: [] }))
      .setTimeout(120)
      .build();
    const prepared = await server.prepareTransaction(built);
    const { signedTxXdr } = await sdk.signer.signTransaction(prepared.toXDR(), { networkPassphrase: sdk.networkPassphrase });
    const sent = await server.sendTransaction(TransactionBuilder.fromXDR(signedTxXdr, sdk.networkPassphrase));
    if (sent.status === "ERROR") throw new Error(`${t.type ?? "tx"} rejected: ${JSON.stringify(sent.errorResult)}`);
    const done = await server.pollTransaction(sent.hash);
    if (done.status !== rpc.Api.GetTransactionStatus.SUCCESS) throw new Error(`${t.type ?? "tx"} ${sent.hash} ${done.status}`);
    hashes.push(sent.hash);
  }
  return hashes;
}

/**
 * One-time maker setup: allow the settlement contract to pay out `amount` of
 * `makerToken`, and optionally register a delegated signing key (required
 * when the maker is a contract account).
 */
export async function prepareMaker(sdk: Octarine, p: { makerToken: string; amount: bigint; ledgers?: number; delegate?: Keypair }) {
  if (!sdk.signer) throw new Error("prepareMaker needs the maker's signer");
  const maker = sdk.signer.publicKey;
  const rfqId = sdk.contracts.rfq!;
  const latest = (await new rpc.Server(sdk.rpcUrl).getLatestLedger()).sequence;
  const out: string[] = [];
  const a = await (await sdk.token(p.makerToken).approve({ from: maker, spender: rfqId, amount: p.amount, expiration_ledger: latest + (p.ledgers ?? 500_000) })).signAndSend();
  out.push(a.sendTransactionResponse?.hash ?? "");
  if (p.delegate) {
    const r = await (await sdk.rfq().register_order_signer({ maker, signer: Buffer.from(p.delegate.rawPublicKey()), allowed: true })).signAndSend();
    out.push(r.sendTransactionResponse?.hash ?? "");
  }
  return out;
}
