import { Buffer } from "buffer";
import { Address } from "@stellar/stellar-sdk";
import {
  AssembledTransaction,
  Client as ContractClient,
  ClientOptions as ContractClientOptions,
  MethodOptions,
  Result,
  Spec as ContractSpec,
} from "@stellar/stellar-sdk/contract";
import type {
  u32,
  i32,
  u64,
  i64,
  u128,
  i128,
  u256,
  i256,
  Option,
  Timepoint,
  Duration,
} from "@stellar/stellar-sdk/contract";






/**
 * One swap on one venue. `data` is the venue adapter's own payload.
 */
export interface Hop {
  data: Buffer;
  token_out: string;
  venue: string;
}


/**
 * One call of a route built off-chain (SimpleSwap-style). `target` must be an
 * allowlisted router; `auth` pre-authorises the `token_in` transfers it makes
 * from this contract, and nothing else.
 */
export interface Call {
  args: Array<any>;
  auth: Array<unknown>;
  function: string;
  target: string;
}

/**
 * A router fill's `data`: a route through adapters, or calldata.
 */
export type Route = {tag: "Splits", values: readonly [Array<Split>]} | {tag: "Calls", values: readonly [Array<Call>]};


/**
 * A slice of the input, routed through `hops` in sequence.
 */
export interface Split {
  amount_in: i128;
  hops: Array<Hop>;
}


/**
 * An integrator taking a fee on swaps it sends through.
 */
export interface Partner {
  address: string;
  fee_bps: u32;
}


export interface FeeConfig {
  max_partner_fee_bps: u32;
  /**
 * The protocol's cut of every partner fee.
 */
partner_share_bps: u32;
  protocol_fee_bps: u32;
  recipient: string;
}


export interface SwapResult {
  /**
 * What the swapper received.
 */
amount_out: i128;
  /**
 * What the route delivered, before fees.
 */
gross_out: i128;
  partner_fee: i128;
  protocol_fee: i128;
}


/**
 * The terms of a direct swap: what goes in, what must come out, and where.
 */
export interface SwapDescription {
  amount_in: i128;
  /**
 * The least the swapper accepts, net of every fee.
 */
min_out: i128;
  to: string;
  token_in: string;
  token_out: string;
}

export const Errors = {
  1: {message:"InvalidConfig"},
  2: {message:"InvalidRoute"},
  3: {message:"VenueNotAllowed"},
  4: {message:"BelowMinOut"},
  5: {message:"InputShortfall"},
  6: {message:"PartnerFeeTooHigh"},
  7: {message:"Paused"},
  8: {message:"TooManyVenues"},
  9: {message:"Overflow"}
}


/**
 * An asset's redemption schedule. The settlement contract keeps one per asset
 * for market makers; a facility keeps its own per RWA it buys.
 */
export interface Schedule {
  cycle_seconds: u32;
  /**
 * Rate ceiling for this asset, in hundredths of a basis point per day:
 * 1000 is 10.00 bps/day.
 */
max_bps_per_day: u32;
  mode: ScheduleMode;
  next_redemption_at: u64;
  rolling_seconds: u32;
}


/**
 * An oracle reading: `price` is quote per base, 1e18-scaled over raw units.
 */
export interface PriceData {
  price: i128;
  timestamp: u64;
}


export interface QueueConfig {
  /**
 * Who deployed it, through the factory.
 */
curator: string;
  facility: string;
}


/**
 * A vault's settings at creation. `asset` never changes.
 */
export interface VaultConfig {
  asset: string;
  curator: string;
  /**
 * Performance fee on share-price gains above the high-water mark.
 */
curator_fee_bps: u32;
  /**
 * NAV ceiling for deposits; 0 means uncapped.
 */
deposit_cap: i128;
  /**
 * May revoke any pending timelocked change.
 */
guardian: string;
  manager: string;
  name: string;
  symbol: string;
  /**
 * Seconds a governed change waits before it can be accepted.
 */
timelock: u64;
}

export type ScheduleMode = {tag: "Rolling", values: void} | {tag: "Cyclical", values: void};


/**
 * A Facility leg's `data`: which facility, and who pays the input. The
 * facility requires the payer's own signature, so naming anyone else fails.
 */
export interface FacilityRoute {
  facility: string;
  payer: string;
}


export interface FacilityConfig {
  /**
 * The vault holding the facility's base asset (its stablecoin).
 */
base_vault: string;
  curator: string;
  /**
 * Performance fee on oLP share-price gains above the high-water mark.
 */
curator_fee_bps: u32;
  guardian: string;
  manager: string;
  name: string;
  symbol: string;
  timelock: u64;
  /**
 * Seconds over which a trade's gain against the oracle mark unlocks.
 */
vest_seconds: u64;
}

export const TimelockError = {
  110: {message:"NoPending"},
  111: {message:"NotReady"},
  112: {message:"NotGuardian"}
}

export interface Client {
  /**
   * Construct and simulate a fees transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  fees: (options?: MethodOptions) => Promise<AssembledTransaction<FeeConfig>>

  /**
   * Construct and simulate a fill transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Router only. The router has already transferred `amount_in`; `data` is
   * a `Route` as XDR. No partner fee in this path.
   */
  fill: ({recipient, token_in, token_out, amount_in, min_amount_out, data}: {recipient: string, token_in: string, token_out: string, amount_in: i128, min_amount_out: i128, data: Buffer}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a owed transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  owed: ({recipient, token}: {recipient: string, token: string}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a swap transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * A direct swap. `user` authorises and pays; `desc.to` receives.
   */
  swap: ({user, desc, route, partner}: {user: string, desc: SwapDescription, route: Array<Split>, partner: Option<Partner>}, options?: MethodOptions) => Promise<AssembledTransaction<SwapResult>>

  /**
   * Construct and simulate a admin transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  admin: (options?: MethodOptions) => Promise<AssembledTransaction<string>>

  /**
   * Construct and simulate a claim transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Pays out everything `recipient` is owed in each of `tokens`.
   */
  claim: ({recipient, tokens}: {recipient: string, tokens: Array<string>}, options?: MethodOptions) => Promise<AssembledTransaction<Array<i128>>>

  /**
   * Construct and simulate a quote transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * The best direct single-hop quote across allowlisted venues, net of the
   * protocol fee. Advisory: real routes are priced with `quote_route`.
   */
  quote: ({token_in, token_out, amount_in}: {token_in: string, token_out: string, amount_in: i128}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a router transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  router: (options?: MethodOptions) => Promise<AssembledTransaction<string>>

  /**
   * Construct and simulate a venues transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  venues: (options?: MethodOptions) => Promise<AssembledTransaction<Array<string>>>

  /**
   * Construct and simulate a targets transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Routers that calldata routes may call.
   */
  targets: (options?: MethodOptions) => Promise<AssembledTransaction<Array<string>>>

  /**
   * Construct and simulate a upgrade transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  upgrade: ({new_wasm_hash}: {new_wasm_hash: Buffer}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a set_fees transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_fees: ({fees}: {fees: FeeConfig}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a is_paused transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  is_paused: (options?: MethodOptions) => Promise<AssembledTransaction<boolean>>

  /**
   * Construct and simulate a set_paused transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Stops swaps. Never stops anyone claiming fees they are owed.
   */
  set_paused: ({paused}: {paused: boolean}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a set_router transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_router: ({router}: {router: string}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a swap_calls transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * A swap along calldata built off-chain. Unspent input goes back to `user`.
   */
  swap_calls: ({user, desc, calls, partner}: {user: string, desc: SwapDescription, calls: Array<Call>, partner: Option<Partner>}, options?: MethodOptions) => Promise<AssembledTransaction<SwapResult>>

  /**
   * Construct and simulate a total_owed transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Fees accrued in `token` and not yet claimed. The aggregator's balance of
   * `token` never falls below this.
   */
  total_owed: ({token}: {token: string}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a allow_venue transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  allow_venue: ({venue, allowed}: {venue: string, allowed: boolean}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a quote_route transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Simulates `route` exactly, net of fees: what the backend shows a user.
   */
  quote_route: ({token_in, token_out, amount_in, route, partner}: {token_in: string, token_out: string, amount_in: i128, route: Array<Split>, partner: Option<Partner>}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a allow_target transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  allow_target: ({target, allowed}: {target: string, allowed: boolean}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

}
export class Client extends ContractClient {
  static async deploy<T = Client>(
        /** Constructor/Initialization Args for the contract's `__constructor` method */
        {admin, router, fees}: {admin: string, router: string, fees: FeeConfig},
    /** Options for initializing a Client as well as for calling a method, with extras specific to deploying. */
    options: MethodOptions &
      Omit<ContractClientOptions, "contractId"> & {
        /** The hash of the Wasm blob, which must already be installed on-chain. */
        wasmHash: Buffer | string;
        /** Salt used to generate the contract's ID. Passed through to {@link Operation.createCustomContract}. Default: random. */
        salt?: Buffer | Uint8Array;
        /** The format used to decode `wasmHash`, if it's provided as a string. */
        format?: "hex" | "base64";
      }
  ): Promise<AssembledTransaction<T>> {
    return ContractClient.deploy({admin, router, fees}, options)
  }
  constructor(public readonly options: ContractClientOptions) {
    super(
      new ContractSpec([ "AAAAAAAAAAAAAAAEZmVlcwAAAAAAAAABAAAH0AAAAAlGZWVDb25maWcAAAA=",
        "AAAAAAAAAHVSb3V0ZXIgb25seS4gVGhlIHJvdXRlciBoYXMgYWxyZWFkeSB0cmFuc2ZlcnJlZCBgYW1vdW50X2luYDsgYGRhdGFgIGlzCmEgYFJvdXRlYCBhcyBYRFIuIE5vIHBhcnRuZXIgZmVlIGluIHRoaXMgcGF0aC4AAAAAAAAEZmlsbAAAAAYAAAAAAAAACXJlY2lwaWVudAAAAAAAABMAAAAAAAAACHRva2VuX2luAAAAEwAAAAAAAAAJdG9rZW5fb3V0AAAAAAAAEwAAAAAAAAAJYW1vdW50X2luAAAAAAAACwAAAAAAAAAObWluX2Ftb3VudF9vdXQAAAAAAAsAAAAAAAAABGRhdGEAAAAOAAAAAQAAAAs=",
        "AAAAAAAAAAAAAAAEb3dlZAAAAAIAAAAAAAAACXJlY2lwaWVudAAAAAAAABMAAAAAAAAABXRva2VuAAAAAAAAEwAAAAEAAAAL",
        "AAAAAAAAAD5BIGRpcmVjdCBzd2FwLiBgdXNlcmAgYXV0aG9yaXNlcyBhbmQgcGF5czsgYGRlc2MudG9gIHJlY2VpdmVzLgAAAAAABHN3YXAAAAAEAAAAAAAAAAR1c2VyAAAAEwAAAAAAAAAEZGVzYwAAB9AAAAAPU3dhcERlc2NyaXB0aW9uAAAAAAAAAAAFcm91dGUAAAAAAAPqAAAH0AAAAAVTcGxpdAAAAAAAAAAAAAAHcGFydG5lcgAAAAPoAAAH0AAAAAdQYXJ0bmVyAAAAAAEAAAfQAAAAClN3YXBSZXN1bHQAAA==",
        "AAAAAAAAAAAAAAAFYWRtaW4AAAAAAAAAAAAAAQAAABM=",
        "AAAAAAAAADxQYXlzIG91dCBldmVyeXRoaW5nIGByZWNpcGllbnRgIGlzIG93ZWQgaW4gZWFjaCBvZiBgdG9rZW5zYC4AAAAFY2xhaW0AAAAAAAACAAAAAAAAAAlyZWNpcGllbnQAAAAAAAATAAAAAAAAAAZ0b2tlbnMAAAAAA+oAAAATAAAAAQAAA+oAAAAL",
        "AAAAAAAAAIlUaGUgYmVzdCBkaXJlY3Qgc2luZ2xlLWhvcCBxdW90ZSBhY3Jvc3MgYWxsb3dsaXN0ZWQgdmVudWVzLCBuZXQgb2YgdGhlCnByb3RvY29sIGZlZS4gQWR2aXNvcnk6IHJlYWwgcm91dGVzIGFyZSBwcmljZWQgd2l0aCBgcXVvdGVfcm91dGVgLgAAAAAAAAVxdW90ZQAAAAAAAAMAAAAAAAAACHRva2VuX2luAAAAEwAAAAAAAAAJdG9rZW5fb3V0AAAAAAAAEwAAAAAAAAAJYW1vdW50X2luAAAAAAAACwAAAAEAAAAL",
        "AAAAAAAAAAAAAAAGcm91dGVyAAAAAAAAAAAAAQAAABM=",
        "AAAAAAAAAAAAAAAGdmVudWVzAAAAAAAAAAAAAQAAA+oAAAAT",
        "AAAAAAAAACZSb3V0ZXJzIHRoYXQgY2FsbGRhdGEgcm91dGVzIG1heSBjYWxsLgAAAAAAB3RhcmdldHMAAAAAAAAAAAEAAAPqAAAAEw==",
        "AAAAAAAAAAAAAAAHdXBncmFkZQAAAAABAAAAAAAAAA1uZXdfd2FzbV9oYXNoAAAAAAAD7gAAACAAAAAA",
        "AAAAAAAAAAAAAAAIc2V0X2ZlZXMAAAABAAAAAAAAAARmZWVzAAAH0AAAAAlGZWVDb25maWcAAAAAAAAA",
        "AAAAAAAAAAAAAAAJaXNfcGF1c2VkAAAAAAAAAAAAAAEAAAAB",
        "AAAAAAAAADxTdG9wcyBzd2Fwcy4gTmV2ZXIgc3RvcHMgYW55b25lIGNsYWltaW5nIGZlZXMgdGhleSBhcmUgb3dlZC4AAAAKc2V0X3BhdXNlZAAAAAAAAQAAAAAAAAAGcGF1c2VkAAAAAAABAAAAAA==",
        "AAAAAAAAAAAAAAAKc2V0X3JvdXRlcgAAAAAAAQAAAAAAAAAGcm91dGVyAAAAAAATAAAAAA==",
        "AAAAAAAAAElBIHN3YXAgYWxvbmcgY2FsbGRhdGEgYnVpbHQgb2ZmLWNoYWluLiBVbnNwZW50IGlucHV0IGdvZXMgYmFjayB0byBgdXNlcmAuAAAAAAAACnN3YXBfY2FsbHMAAAAAAAQAAAAAAAAABHVzZXIAAAATAAAAAAAAAARkZXNjAAAH0AAAAA9Td2FwRGVzY3JpcHRpb24AAAAAAAAAAAVjYWxscwAAAAAAA+oAAAfQAAAABENhbGwAAAAAAAAAB3BhcnRuZXIAAAAD6AAAB9AAAAAHUGFydG5lcgAAAAABAAAH0AAAAApTd2FwUmVzdWx0AAA=",
        "AAAAAAAAAGhGZWVzIGFjY3J1ZWQgaW4gYHRva2VuYCBhbmQgbm90IHlldCBjbGFpbWVkLiBUaGUgYWdncmVnYXRvcidzIGJhbGFuY2Ugb2YKYHRva2VuYCBuZXZlciBmYWxscyBiZWxvdyB0aGlzLgAAAAp0b3RhbF9vd2VkAAAAAAABAAAAAAAAAAV0b2tlbgAAAAAAABMAAAABAAAACw==",
        "AAAAAAAAAAAAAAALYWxsb3dfdmVudWUAAAAAAgAAAAAAAAAFdmVudWUAAAAAAAATAAAAAAAAAAdhbGxvd2VkAAAAAAEAAAAA",
        "AAAAAAAAAEZTaW11bGF0ZXMgYHJvdXRlYCBleGFjdGx5LCBuZXQgb2YgZmVlczogd2hhdCB0aGUgYmFja2VuZCBzaG93cyBhIHVzZXIuAAAAAAALcXVvdGVfcm91dGUAAAAABQAAAAAAAAAIdG9rZW5faW4AAAATAAAAAAAAAAl0b2tlbl9vdXQAAAAAAAATAAAAAAAAAAlhbW91bnRfaW4AAAAAAAALAAAAAAAAAAVyb3V0ZQAAAAAAA+oAAAfQAAAABVNwbGl0AAAAAAAAAAAAAAdwYXJ0bmVyAAAAA+gAAAfQAAAAB1BhcnRuZXIAAAAAAQAAAAs=",
        "AAAAAAAAAAAAAAAMYWxsb3dfdGFyZ2V0AAAAAgAAAAAAAAAGdGFyZ2V0AAAAAAATAAAAAAAAAAdhbGxvd2VkAAAAAAEAAAAA",
        "AAAAAAAAAAAAAAANX19jb25zdHJ1Y3RvcgAAAAAAAAMAAAAAAAAABWFkbWluAAAAAAAAEwAAAAAAAAAGcm91dGVyAAAAAAATAAAAAAAAAARmZWVzAAAH0AAAAAlGZWVDb25maWcAAAAAAAAA",
        "AAAAAQAAAEFPbmUgc3dhcCBvbiBvbmUgdmVudWUuIGBkYXRhYCBpcyB0aGUgdmVudWUgYWRhcHRlcidzIG93biBwYXlsb2FkLgAAAAAAAAAAAAADSG9wAAAAAAMAAAAAAAAABGRhdGEAAAAOAAAAAAAAAAl0b2tlbl9vdXQAAAAAAAATAAAAAAAAAAV2ZW51ZQAAAAAAABM=",
        "AAAAAQAAAL1PbmUgY2FsbCBvZiBhIHJvdXRlIGJ1aWx0IG9mZi1jaGFpbiAoU2ltcGxlU3dhcC1zdHlsZSkuIGB0YXJnZXRgIG11c3QgYmUgYW4KYWxsb3dsaXN0ZWQgcm91dGVyOyBgYXV0aGAgcHJlLWF1dGhvcmlzZXMgdGhlIGB0b2tlbl9pbmAgdHJhbnNmZXJzIGl0IG1ha2VzCmZyb20gdGhpcyBjb250cmFjdCwgYW5kIG5vdGhpbmcgZWxzZS4AAAAAAAAAAAAABENhbGwAAAAEAAAAAAAAAARhcmdzAAAD6gAAAAAAAAAAAAAABGF1dGgAAAPqAAAH0AAAABhJbnZva2VyQ29udHJhY3RBdXRoRW50cnkAAAAAAAAACGZ1bmN0aW9uAAAAEQAAAAAAAAAGdGFyZ2V0AAAAAAAT",
        "AAAAAgAAAD5BIHJvdXRlciBmaWxsJ3MgYGRhdGFgOiBhIHJvdXRlIHRocm91Z2ggYWRhcHRlcnMsIG9yIGNhbGxkYXRhLgAAAAAAAAAAAAVSb3V0ZQAAAAAAAAIAAAABAAAAAAAAAAZTcGxpdHMAAAAAAAEAAAPqAAAH0AAAAAVTcGxpdAAAAAAAAAEAAAAAAAAABUNhbGxzAAAAAAAAAQAAA+oAAAfQAAAABENhbGw=",
        "AAAAAQAAADhBIHNsaWNlIG9mIHRoZSBpbnB1dCwgcm91dGVkIHRocm91Z2ggYGhvcHNgIGluIHNlcXVlbmNlLgAAAAAAAAAFU3BsaXQAAAAAAAACAAAAAAAAAAlhbW91bnRfaW4AAAAAAAALAAAAAAAAAARob3BzAAAD6gAAB9AAAAADSG9wAA==",
        "AAAAAQAAADVBbiBpbnRlZ3JhdG9yIHRha2luZyBhIGZlZSBvbiBzd2FwcyBpdCBzZW5kcyB0aHJvdWdoLgAAAAAAAAAAAAAHUGFydG5lcgAAAAACAAAAAAAAAAdhZGRyZXNzAAAAABMAAAAAAAAAB2ZlZV9icHMAAAAABA==",
        "AAAAAQAAAAAAAAAAAAAACUZlZUNvbmZpZwAAAAAAAAQAAAAAAAAAE21heF9wYXJ0bmVyX2ZlZV9icHMAAAAABAAAAChUaGUgcHJvdG9jb2wncyBjdXQgb2YgZXZlcnkgcGFydG5lciBmZWUuAAAAEXBhcnRuZXJfc2hhcmVfYnBzAAAAAAAABAAAAAAAAAAQcHJvdG9jb2xfZmVlX2JwcwAAAAQAAAAAAAAACXJlY2lwaWVudAAAAAAAABM=",
        "AAAAAQAAAAAAAAAAAAAAClN3YXBSZXN1bHQAAAAAAAQAAAAaV2hhdCB0aGUgc3dhcHBlciByZWNlaXZlZC4AAAAAAAphbW91bnRfb3V0AAAAAAALAAAAJldoYXQgdGhlIHJvdXRlIGRlbGl2ZXJlZCwgYmVmb3JlIGZlZXMuAAAAAAAJZ3Jvc3Nfb3V0AAAAAAAACwAAAAAAAAALcGFydG5lcl9mZWUAAAAACwAAAAAAAAAMcHJvdG9jb2xfZmVlAAAACw==",
        "AAAAAQAAAEhUaGUgdGVybXMgb2YgYSBkaXJlY3Qgc3dhcDogd2hhdCBnb2VzIGluLCB3aGF0IG11c3QgY29tZSBvdXQsIGFuZCB3aGVyZS4AAAAAAAAAD1N3YXBEZXNjcmlwdGlvbgAAAAAFAAAAAAAAAAlhbW91bnRfaW4AAAAAAAALAAAAMFRoZSBsZWFzdCB0aGUgc3dhcHBlciBhY2NlcHRzLCBuZXQgb2YgZXZlcnkgZmVlLgAAAAdtaW5fb3V0AAAAAAsAAAAAAAAAAnRvAAAAAAATAAAAAAAAAAh0b2tlbl9pbgAAABMAAAAAAAAACXRva2VuX291dAAAAAAAABM=",
        "AAAABAAAAAAAAAAAAAAABUVycm9yAAAAAAAACQAAAAAAAAANSW52YWxpZENvbmZpZwAAAAAAAAEAAAAAAAAADEludmFsaWRSb3V0ZQAAAAIAAAAAAAAAD1ZlbnVlTm90QWxsb3dlZAAAAAADAAAAAAAAAAtCZWxvd01pbk91dAAAAAAEAAAAAAAAAA5JbnB1dFNob3J0ZmFsbAAAAAAABQAAAAAAAAARUGFydG5lckZlZVRvb0hpZ2gAAAAAAAAGAAAAAAAAAAZQYXVzZWQAAAAAAAcAAAAAAAAADVRvb01hbnlWZW51ZXMAAAAAAAAIAAAAAAAAAAhPdmVyZmxvdwAAAAk=",
        "AAAAAQAAAIhBbiBhc3NldCdzIHJlZGVtcHRpb24gc2NoZWR1bGUuIFRoZSBzZXR0bGVtZW50IGNvbnRyYWN0IGtlZXBzIG9uZSBwZXIgYXNzZXQKZm9yIG1hcmtldCBtYWtlcnM7IGEgZmFjaWxpdHkga2VlcHMgaXRzIG93biBwZXIgUldBIGl0IGJ1eXMuAAAAAAAAAAhTY2hlZHVsZQAAAAUAAAAAAAAADWN5Y2xlX3NlY29uZHMAAAAAAAAEAAAAW1JhdGUgY2VpbGluZyBmb3IgdGhpcyBhc3NldCwgaW4gaHVuZHJlZHRocyBvZiBhIGJhc2lzIHBvaW50IHBlciBkYXk6CjEwMDAgaXMgMTAuMDAgYnBzL2RheS4AAAAAD21heF9icHNfcGVyX2RheQAAAAAEAAAAAAAAAARtb2RlAAAH0AAAAAxTY2hlZHVsZU1vZGUAAAAAAAAAEm5leHRfcmVkZW1wdGlvbl9hdAAAAAAABgAAAAAAAAAPcm9sbGluZ19zZWNvbmRzAAAAAAQ=",
        "AAAAAQAAAElBbiBvcmFjbGUgcmVhZGluZzogYHByaWNlYCBpcyBxdW90ZSBwZXIgYmFzZSwgMWUxOC1zY2FsZWQgb3ZlciByYXcgdW5pdHMuAAAAAAAAAAAAAAlQcmljZURhdGEAAAAAAAACAAAAAAAAAAVwcmljZQAAAAAAAAsAAAAAAAAACXRpbWVzdGFtcAAAAAAAAAY=",
        "AAAAAQAAAAAAAAAAAAAAC1F1ZXVlQ29uZmlnAAAAAAIAAAAlV2hvIGRlcGxveWVkIGl0LCB0aHJvdWdoIHRoZSBmYWN0b3J5LgAAAAAAAAdjdXJhdG9yAAAAABMAAAAAAAAACGZhY2lsaXR5AAAAEw==",
        "AAAAAQAAADZBIHZhdWx0J3Mgc2V0dGluZ3MgYXQgY3JlYXRpb24uIGBhc3NldGAgbmV2ZXIgY2hhbmdlcy4AAAAAAAAAAAALVmF1bHRDb25maWcAAAAACQAAAAAAAAAFYXNzZXQAAAAAAAATAAAAAAAAAAdjdXJhdG9yAAAAABMAAAA/UGVyZm9ybWFuY2UgZmVlIG9uIHNoYXJlLXByaWNlIGdhaW5zIGFib3ZlIHRoZSBoaWdoLXdhdGVyIG1hcmsuAAAAAA9jdXJhdG9yX2ZlZV9icHMAAAAABAAAACtOQVYgY2VpbGluZyBmb3IgZGVwb3NpdHM7IDAgbWVhbnMgdW5jYXBwZWQuAAAAAAtkZXBvc2l0X2NhcAAAAAALAAAAKU1heSByZXZva2UgYW55IHBlbmRpbmcgdGltZWxvY2tlZCBjaGFuZ2UuAAAAAAAACGd1YXJkaWFuAAAAEwAAAAAAAAAHbWFuYWdlcgAAAAATAAAAAAAAAARuYW1lAAAAEAAAAAAAAAAGc3ltYm9sAAAAAAAQAAAAOlNlY29uZHMgYSBnb3Zlcm5lZCBjaGFuZ2Ugd2FpdHMgYmVmb3JlIGl0IGNhbiBiZSBhY2NlcHRlZC4AAAAAAAh0aW1lbG9jawAAAAY=",
        "AAAAAgAAAAAAAAAAAAAADFNjaGVkdWxlTW9kZQAAAAIAAAAAAAAANUFsd2F5cyBUK046IGByb2xsaW5nX3NlY29uZHNgIHJlZ2FyZGxlc3Mgb2YgdGhlIGRhdGUuAAAAAAAAB1JvbGxpbmcAAAAAAAAAAEVSZWRlbXB0aW9uIGxhbmRzIG9uIGEgY2FsZW5kYXIgZGF0ZSBhbmQgcm9sbHMgZm9yd2FyZCBvbmNlIGl0IHBhc3Nlcy4AAAAAAAAIQ3ljbGljYWw=",
        "AAAAAQAAAI5BIEZhY2lsaXR5IGxlZydzIGBkYXRhYDogd2hpY2ggZmFjaWxpdHksIGFuZCB3aG8gcGF5cyB0aGUgaW5wdXQuIFRoZQpmYWNpbGl0eSByZXF1aXJlcyB0aGUgcGF5ZXIncyBvd24gc2lnbmF0dXJlLCBzbyBuYW1pbmcgYW55b25lIGVsc2UgZmFpbHMuAAAAAAAAAAAADUZhY2lsaXR5Um91dGUAAAAAAAACAAAAAAAAAAhmYWNpbGl0eQAAABMAAAAAAAAABXBheWVyAAAAAAAAEw==",
        "AAAAAQAAAAAAAAAAAAAADkZhY2lsaXR5Q29uZmlnAAAAAAAJAAAAPVRoZSB2YXVsdCBob2xkaW5nIHRoZSBmYWNpbGl0eSdzIGJhc2UgYXNzZXQgKGl0cyBzdGFibGVjb2luKS4AAAAAAAAKYmFzZV92YXVsdAAAAAAAEwAAAAAAAAAHY3VyYXRvcgAAAAATAAAAQ1BlcmZvcm1hbmNlIGZlZSBvbiBvTFAgc2hhcmUtcHJpY2UgZ2FpbnMgYWJvdmUgdGhlIGhpZ2gtd2F0ZXIgbWFyay4AAAAAD2N1cmF0b3JfZmVlX2JwcwAAAAAEAAAAAAAAAAhndWFyZGlhbgAAABMAAAAAAAAAB21hbmFnZXIAAAAAEwAAAAAAAAAEbmFtZQAAABAAAAAAAAAABnN5bWJvbAAAAAAAEAAAAAAAAAAIdGltZWxvY2sAAAAGAAAAQlNlY29uZHMgb3ZlciB3aGljaCBhIHRyYWRlJ3MgZ2FpbiBhZ2FpbnN0IHRoZSBvcmFjbGUgbWFyayB1bmxvY2tzLgAAAAAADHZlc3Rfc2Vjb25kcwAAAAY=",
        "AAAABAAAAAAAAAAAAAAADVRpbWVsb2NrRXJyb3IAAAAAAAADAAAAAAAAAAlOb1BlbmRpbmcAAAAAAABuAAAAAAAAAAhOb3RSZWFkeQAAAG8AAAAAAAAAC05vdEd1YXJkaWFuAAAAAHA=" ]),
      options
    )
  }
  public readonly fromJSON = {
    fees: this.txFromJSON<FeeConfig>,
        fill: this.txFromJSON<i128>,
        owed: this.txFromJSON<i128>,
        swap: this.txFromJSON<SwapResult>,
        admin: this.txFromJSON<string>,
        claim: this.txFromJSON<Array<i128>>,
        quote: this.txFromJSON<i128>,
        router: this.txFromJSON<string>,
        venues: this.txFromJSON<Array<string>>,
        targets: this.txFromJSON<Array<string>>,
        upgrade: this.txFromJSON<null>,
        set_fees: this.txFromJSON<null>,
        is_paused: this.txFromJSON<boolean>,
        set_paused: this.txFromJSON<null>,
        set_router: this.txFromJSON<null>,
        swap_calls: this.txFromJSON<SwapResult>,
        total_owed: this.txFromJSON<i128>,
        allow_venue: this.txFromJSON<null>,
        quote_route: this.txFromJSON<i128>,
        allow_target: this.txFromJSON<null>
  }
}