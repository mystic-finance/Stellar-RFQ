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






export interface Rwa {
  terms: Terms;
  /**
 * The vault holding this RWA; the address the issuer allowlists.
 */
vault: string;
}


/**
 * How the facility trades one RWA. Rates are hundredths of a basis point per
 * day over the RWA's horizon: it buys at `bid_rate` and sells at `ask_rate`.
 */
export interface Terms {
  accepted: boolean;
  ask_rate: u32;
  bid_rate: u32;
  /**
 * Ceiling on this RWA's oracle value as a share of NAV, in bps.
 */
max_bps: u32;
  max_price_age: u64;
  /**
 * Price of the RWA in the base asset (`get_price(rwa, base)`).
 */
oracle: string;
  /**
 * Where booked RWA goes: the issuer's redemption account or custody.
 */
redemption: string;
  schedule: Schedule;
}

/**
 * Changes that wait out the timelock.
 */
export type Change = {tag: "CuratorFee", values: readonly [u32]} | {tag: "Vest", values: readonly [u64]} | {tag: "Queue", values: readonly [string]} | {tag: "Rwa", values: readonly [string, string, Terms]};


/**
 * RWA out with its issuer, carried at its oracle value when booked.
 */
export interface Redemption {
  amount: i128;
  carried: i128;
}

export const Errors = {
  1: {message:"InvalidAmount"},
  2: {message:"InvalidConfig"},
  3: {message:"UnsupportedPair"},
  4: {message:"NotAccepted"},
  5: {message:"NoPrice"},
  6: {message:"CapExceeded"},
  7: {message:"InsufficientLiquidity"},
  8: {message:"BelowMinOut"},
  9: {message:"NotAllowlisted"},
  10: {message:"TooManyRwas"},
  11: {message:"StillHeld"},
  12: {message:"RedemptionOpen"},
  13: {message:"NoRedemption"},
  14: {message:"NotFullyRepaid"},
  15: {message:"Paused"}
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


export interface Meta {
  decimals: u32;
  name: string;
  symbol: string;
}

export const ShareError = {
  100: {message:"InvalidAmount"},
  101: {message:"InsufficientBalance"},
  102: {message:"InsufficientAllowance"},
  103: {message:"NotAllowlisted"},
  104: {message:"Frozen"},
  105: {message:"Overflow"}
}

export interface Client {
  /**
   * Construct and simulate a rwa transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  rwa: ({rwa}: {rwa: string}, options?: MethodOptions) => Promise<AssembledTransaction<Option<Rwa>>>

  /**
   * Construct and simulate a base transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  base: (options?: MethodOptions) => Promise<AssembledTransaction<string>>

  /**
   * Construct and simulate a book transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Sends facility-owned RWA to its redemption address; NAV carries it as a
   * receivable at its oracle value, so the share price does not move.
   */
  book: ({rwa, amount}: {rwa: string, amount: i128}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a burn transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  burn: ({from, amount}: {from: string, amount: i128}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a name transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  name: (options?: MethodOptions) => Promise<AssembledTransaction<string>>

  /**
   * Construct and simulate a rwas transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  rwas: (options?: MethodOptions) => Promise<AssembledTransaction<Array<string>>>

  /**
   * Construct and simulate a queue transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  queue: (options?: MethodOptions) => Promise<AssembledTransaction<Option<string>>>

  /**
   * Construct and simulate a quote transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * What `trade` would pay for `amount_in`, or 0 if it would not fill.
   */
  quote: ({token_in, token_out, amount_in}: {token_in: string, token_out: string, amount_in: i128}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a trade transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * `payer` pays `amount_in` of `token_in` straight into its vault, and the
   * output vault pays `recipient`. Any caller may route it; the payer signs.
   */
  trade: ({payer, recipient, token_in, token_out, amount_in, min_out}: {payer: string, recipient: string, token_in: string, token_out: string, amount_in: i128, min_out: i128}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a accept transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  accept: ({kind}: {kind: u32}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a accrue transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  accrue: (options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a config transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  config: (options?: MethodOptions) => Promise<AssembledTransaction<FacilityConfig>>

  /**
   * Construct and simulate a locked transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Gain against the oracle mark not yet released to oLP holders.
   */
  locked: (options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a paused transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  paused: (options?: MethodOptions) => Promise<AssembledTransaction<boolean>>

  /**
   * Construct and simulate a redeem transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  redeem: ({shares, receiver, owner, operator}: {shares: i128, receiver: string, owner: string, operator: string}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a revoke transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  revoke: ({kind, caller}: {kind: u32, caller: string}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a settle transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * All-or-nothing: less than the carried value reverts. A surplus vests.
   */
  settle: ({rwa, proceeds}: {rwa: string, proceeds: i128}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a submit transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the change's kind (to accept or revoke it) and when it unlocks.
   */
  submit: ({change}: {change: Change}, options?: MethodOptions) => Promise<AssembledTransaction<readonly [u32, u64]>>

  /**
   * Construct and simulate a symbol transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  symbol: (options?: MethodOptions) => Promise<AssembledTransaction<string>>

  /**
   * Construct and simulate a approve transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  approve: ({from, spender, amount, expiration_ledger}: {from: string, spender: string, amount: i128, expiration_ledger: u32}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a balance transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  balance: ({id}: {id: string}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a deposit transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  deposit: ({assets, receiver, from, operator}: {assets: i128, receiver: string, from: string, operator: string}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a pending transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  pending: ({kind}: {kind: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Option<readonly [Change, u64]>>>

  /**
   * Construct and simulate a decimals transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  decimals: (options?: MethodOptions) => Promise<AssembledTransaction<u32>>

  /**
   * Construct and simulate a transfer transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  transfer: ({from, to, amount}: {from: string, to: string, amount: i128}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a withdraw transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  withdraw: ({assets, receiver, owner, operator}: {assets: i128, receiver: string, owner: string, operator: string}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a allowance transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  allowance: ({from, spender}: {from: string, spender: string}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a burn_from transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  burn_from: ({spender, from, amount}: {spender: string, from: string, amount: i128}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a base_vault transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  base_vault: (options?: MethodOptions) => Promise<AssembledTransaction<string>>

  /**
   * Construct and simulate a redemption transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  redemption: ({rwa}: {rwa: string}, options?: MethodOptions) => Promise<AssembledTransaction<Option<Redemption>>>

  /**
   * Construct and simulate a remove_rwa transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Drops an RWA the facility no longer holds or is redeeming.
   */
  remove_rwa: ({rwa}: {rwa: string}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a set_member transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_member: ({who, allowed}: {who: string, allowed: boolean}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a set_paused transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Curator or guardian, no timelock: stops trading, never exits.
   */
  set_paused: ({caller, paused}: {caller: string, paused: boolean}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a write_down transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Protocol admin only: books a redemption the issuer will not honour in
   * full, recovering `recovered` (possibly 0).
   */
  write_down: ({rwa, recovered}: {rwa: string, recovered: i128}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a max_withdraw transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  max_withdraw: ({owner}: {owner: string}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a total_assets transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  total_assets: (options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a total_supply transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  total_supply: (options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a set_allowlist transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_allowlist: ({enabled}: {enabled: boolean}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a transfer_from transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  transfer_from: ({spender, from, to, amount}: {spender: string, from: string, to: string, amount: i128}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a convert_to_assets transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  convert_to_assets: ({shares}: {shares: i128}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a convert_to_shares transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  convert_to_shares: ({assets}: {assets: i128}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

}
export class Client extends ContractClient {
  static async deploy<T = Client>(
        /** Constructor/Initialization Args for the contract's `__constructor` method */
        {host, config}: {host: string, config: FacilityConfig},
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
    return ContractClient.deploy({host, config}, options)
  }
  constructor(public readonly options: ContractClientOptions) {
    super(
      new ContractSpec([ "AAAAAAAAAAAAAAADcndhAAAAAAEAAAAAAAAAA3J3YQAAAAATAAAAAQAAA+gAAAfQAAAAA1J3YQA=",
        "AAAAAAAAAAAAAAAEYmFzZQAAAAAAAAABAAAAEw==",
        "AAAAAAAAAIlTZW5kcyBmYWNpbGl0eS1vd25lZCBSV0EgdG8gaXRzIHJlZGVtcHRpb24gYWRkcmVzczsgTkFWIGNhcnJpZXMgaXQgYXMgYQpyZWNlaXZhYmxlIGF0IGl0cyBvcmFjbGUgdmFsdWUsIHNvIHRoZSBzaGFyZSBwcmljZSBkb2VzIG5vdCBtb3ZlLgAAAAAAAARib29rAAAAAgAAAAAAAAADcndhAAAAABMAAAAAAAAABmFtb3VudAAAAAAACwAAAAA=",
        "AAAAAAAAAAAAAAAEYnVybgAAAAIAAAAAAAAABGZyb20AAAATAAAAAAAAAAZhbW91bnQAAAAAAAsAAAAA",
        "AAAAAAAAAAAAAAAEbmFtZQAAAAAAAAABAAAAEA==",
        "AAAAAAAAAAAAAAAEcndhcwAAAAAAAAABAAAD6gAAABM=",
        "AAAAAAAAAAAAAAAFcXVldWUAAAAAAAAAAAAAAQAAA+gAAAAT",
        "AAAAAAAAAEJXaGF0IGB0cmFkZWAgd291bGQgcGF5IGZvciBgYW1vdW50X2luYCwgb3IgMCBpZiBpdCB3b3VsZCBub3QgZmlsbC4AAAAAAAVxdW90ZQAAAAAAAAMAAAAAAAAACHRva2VuX2luAAAAEwAAAAAAAAAJdG9rZW5fb3V0AAAAAAAAEwAAAAAAAAAJYW1vdW50X2luAAAAAAAACwAAAAEAAAAL",
        "AAAAAAAAAJBgcGF5ZXJgIHBheXMgYGFtb3VudF9pbmAgb2YgYHRva2VuX2luYCBzdHJhaWdodCBpbnRvIGl0cyB2YXVsdCwgYW5kIHRoZQpvdXRwdXQgdmF1bHQgcGF5cyBgcmVjaXBpZW50YC4gQW55IGNhbGxlciBtYXkgcm91dGUgaXQ7IHRoZSBwYXllciBzaWducy4AAAAFdHJhZGUAAAAAAAAGAAAAAAAAAAVwYXllcgAAAAAAABMAAAAAAAAACXJlY2lwaWVudAAAAAAAABMAAAAAAAAACHRva2VuX2luAAAAEwAAAAAAAAAJdG9rZW5fb3V0AAAAAAAAEwAAAAAAAAAJYW1vdW50X2luAAAAAAAACwAAAAAAAAAHbWluX291dAAAAAALAAAAAQAAAAs=",
        "AAAAAAAAAAAAAAAGYWNjZXB0AAAAAAABAAAAAAAAAARraW5kAAAABAAAAAA=",
        "AAAAAAAAAAAAAAAGYWNjcnVlAAAAAAAAAAAAAA==",
        "AAAAAAAAAAAAAAAGY29uZmlnAAAAAAAAAAAAAQAAB9AAAAAORmFjaWxpdHlDb25maWcAAA==",
        "AAAAAAAAAD1HYWluIGFnYWluc3QgdGhlIG9yYWNsZSBtYXJrIG5vdCB5ZXQgcmVsZWFzZWQgdG8gb0xQIGhvbGRlcnMuAAAAAAAABmxvY2tlZAAAAAAAAAAAAAEAAAAL",
        "AAAAAAAAAAAAAAAGcGF1c2VkAAAAAAAAAAAAAQAAAAE=",
        "AAAAAAAAAAAAAAAGcmVkZWVtAAAAAAAEAAAAAAAAAAZzaGFyZXMAAAAAAAsAAAAAAAAACHJlY2VpdmVyAAAAEwAAAAAAAAAFb3duZXIAAAAAAAATAAAAAAAAAAhvcGVyYXRvcgAAABMAAAABAAAACw==",
        "AAAAAAAAAAAAAAAGcmV2b2tlAAAAAAACAAAAAAAAAARraW5kAAAABAAAAAAAAAAGY2FsbGVyAAAAAAATAAAAAA==",
        "AAAAAAAAAEVBbGwtb3Itbm90aGluZzogbGVzcyB0aGFuIHRoZSBjYXJyaWVkIHZhbHVlIHJldmVydHMuIEEgc3VycGx1cyB2ZXN0cy4AAAAAAAAGc2V0dGxlAAAAAAACAAAAAAAAAANyd2EAAAAAEwAAAAAAAAAIcHJvY2VlZHMAAAALAAAAAA==",
        "AAAAAAAAAEdSZXR1cm5zIHRoZSBjaGFuZ2UncyBraW5kICh0byBhY2NlcHQgb3IgcmV2b2tlIGl0KSBhbmQgd2hlbiBpdCB1bmxvY2tzLgAAAAAGc3VibWl0AAAAAAABAAAAAAAAAAZjaGFuZ2UAAAAAB9AAAAAGQ2hhbmdlAAAAAAABAAAD7QAAAAIAAAAEAAAABg==",
        "AAAAAAAAAAAAAAAGc3ltYm9sAAAAAAAAAAAAAQAAABA=",
        "AAAAAAAAAAAAAAAHYXBwcm92ZQAAAAAEAAAAAAAAAARmcm9tAAAAEwAAAAAAAAAHc3BlbmRlcgAAAAATAAAAAAAAAAZhbW91bnQAAAAAAAsAAAAAAAAAEWV4cGlyYXRpb25fbGVkZ2VyAAAAAAAABAAAAAA=",
        "AAAAAAAAAAAAAAAHYmFsYW5jZQAAAAABAAAAAAAAAAJpZAAAAAAAEwAAAAEAAAAL",
        "AAAAAAAAAAAAAAAHZGVwb3NpdAAAAAAEAAAAAAAAAAZhc3NldHMAAAAAAAsAAAAAAAAACHJlY2VpdmVyAAAAEwAAAAAAAAAEZnJvbQAAABMAAAAAAAAACG9wZXJhdG9yAAAAEwAAAAEAAAAL",
        "AAAAAAAAAAAAAAAHcGVuZGluZwAAAAABAAAAAAAAAARraW5kAAAABAAAAAEAAAPoAAAD7QAAAAIAAAfQAAAABkNoYW5nZQAAAAAABg==",
        "AAAAAAAAAAAAAAAIZGVjaW1hbHMAAAAAAAAAAQAAAAQ=",
        "AAAAAAAAAAAAAAAIdHJhbnNmZXIAAAADAAAAAAAAAARmcm9tAAAAEwAAAAAAAAACdG8AAAAAABMAAAAAAAAABmFtb3VudAAAAAAACwAAAAA=",
        "AAAAAAAAAAAAAAAId2l0aGRyYXcAAAAEAAAAAAAAAAZhc3NldHMAAAAAAAsAAAAAAAAACHJlY2VpdmVyAAAAEwAAAAAAAAAFb3duZXIAAAAAAAATAAAAAAAAAAhvcGVyYXRvcgAAABMAAAABAAAACw==",
        "AAAAAAAAAAAAAAAJYWxsb3dhbmNlAAAAAAAAAgAAAAAAAAAEZnJvbQAAABMAAAAAAAAAB3NwZW5kZXIAAAAAEwAAAAEAAAAL",
        "AAAAAAAAAAAAAAAJYnVybl9mcm9tAAAAAAAAAwAAAAAAAAAHc3BlbmRlcgAAAAATAAAAAAAAAARmcm9tAAAAEwAAAAAAAAAGYW1vdW50AAAAAAALAAAAAA==",
        "AAAAAAAAAAAAAAAKYmFzZV92YXVsdAAAAAAAAAAAAAEAAAAT",
        "AAAAAAAAAAAAAAAKcmVkZW1wdGlvbgAAAAAAAQAAAAAAAAADcndhAAAAABMAAAABAAAD6AAAB9AAAAAKUmVkZW1wdGlvbgAA",
        "AAAAAAAAADpEcm9wcyBhbiBSV0EgdGhlIGZhY2lsaXR5IG5vIGxvbmdlciBob2xkcyBvciBpcyByZWRlZW1pbmcuAAAAAAAKcmVtb3ZlX3J3YQAAAAAAAQAAAAAAAAADcndhAAAAABMAAAAA",
        "AAAAAAAAAAAAAAAKc2V0X21lbWJlcgAAAAAAAgAAAAAAAAADd2hvAAAAABMAAAAAAAAAB2FsbG93ZWQAAAAAAQAAAAA=",
        "AAAAAAAAAD1DdXJhdG9yIG9yIGd1YXJkaWFuLCBubyB0aW1lbG9jazogc3RvcHMgdHJhZGluZywgbmV2ZXIgZXhpdHMuAAAAAAAACnNldF9wYXVzZWQAAAAAAAIAAAAAAAAABmNhbGxlcgAAAAAAEwAAAAAAAAAGcGF1c2VkAAAAAAABAAAAAA==",
        "AAAAAAAAAHBQcm90b2NvbCBhZG1pbiBvbmx5OiBib29rcyBhIHJlZGVtcHRpb24gdGhlIGlzc3VlciB3aWxsIG5vdCBob25vdXIgaW4KZnVsbCwgcmVjb3ZlcmluZyBgcmVjb3ZlcmVkYCAocG9zc2libHkgMCkuAAAACndyaXRlX2Rvd24AAAAAAAIAAAAAAAAAA3J3YQAAAAATAAAAAAAAAAlyZWNvdmVyZWQAAAAAAAALAAAAAA==",
        "AAAAAAAAAAAAAAAMbWF4X3dpdGhkcmF3AAAAAQAAAAAAAAAFb3duZXIAAAAAAAATAAAAAQAAAAs=",
        "AAAAAAAAAAAAAAAMdG90YWxfYXNzZXRzAAAAAAAAAAEAAAAL",
        "AAAAAAAAAAAAAAAMdG90YWxfc3VwcGx5AAAAAAAAAAEAAAAL",
        "AAAAAAAAAAAAAAANc2V0X2FsbG93bGlzdAAAAAAAAAEAAAAAAAAAB2VuYWJsZWQAAAAAAQAAAAA=",
        "AAAAAAAAAAAAAAANdHJhbnNmZXJfZnJvbQAAAAAAAAQAAAAAAAAAB3NwZW5kZXIAAAAAEwAAAAAAAAAEZnJvbQAAABMAAAAAAAAAAnRvAAAAAAATAAAAAAAAAAZhbW91bnQAAAAAAAsAAAAA",
        "AAAAAAAAAAAAAAANX19jb25zdHJ1Y3RvcgAAAAAAAAIAAAAAAAAABGhvc3QAAAATAAAAAAAAAAZjb25maWcAAAAAB9AAAAAORmFjaWxpdHlDb25maWcAAAAAAAA=",
        "AAAAAAAAAAAAAAARY29udmVydF90b19hc3NldHMAAAAAAAABAAAAAAAAAAZzaGFyZXMAAAAAAAsAAAABAAAACw==",
        "AAAAAAAAAAAAAAARY29udmVydF90b19zaGFyZXMAAAAAAAABAAAAAAAAAAZhc3NldHMAAAAAAAsAAAABAAAACw==",
        "AAAAAQAAAAAAAAAAAAAAA1J3YQAAAAACAAAAAAAAAAV0ZXJtcwAAAAAAB9AAAAAFVGVybXMAAAAAAAA+VGhlIHZhdWx0IGhvbGRpbmcgdGhpcyBSV0E7IHRoZSBhZGRyZXNzIHRoZSBpc3N1ZXIgYWxsb3dsaXN0cy4AAAAAAAV2YXVsdAAAAAAAABM=",
        "AAAAAQAAAJVIb3cgdGhlIGZhY2lsaXR5IHRyYWRlcyBvbmUgUldBLiBSYXRlcyBhcmUgaHVuZHJlZHRocyBvZiBhIGJhc2lzIHBvaW50IHBlcgpkYXkgb3ZlciB0aGUgUldBJ3MgaG9yaXpvbjogaXQgYnV5cyBhdCBgYmlkX3JhdGVgIGFuZCBzZWxscyBhdCBgYXNrX3JhdGVgLgAAAAAAAAAAAAAFVGVybXMAAAAAAAAIAAAAAAAAAAhhY2NlcHRlZAAAAAEAAAAAAAAACGFza19yYXRlAAAABAAAAAAAAAAIYmlkX3JhdGUAAAAEAAAAPUNlaWxpbmcgb24gdGhpcyBSV0EncyBvcmFjbGUgdmFsdWUgYXMgYSBzaGFyZSBvZiBOQVYsIGluIGJwcy4AAAAAAAAHbWF4X2JwcwAAAAAEAAAAAAAAAA1tYXhfcHJpY2VfYWdlAAAAAAAABgAAADxQcmljZSBvZiB0aGUgUldBIGluIHRoZSBiYXNlIGFzc2V0IChgZ2V0X3ByaWNlKHJ3YSwgYmFzZSlgKS4AAAAGb3JhY2xlAAAAAAATAAAAQldoZXJlIGJvb2tlZCBSV0EgZ29lczogdGhlIGlzc3VlcidzIHJlZGVtcHRpb24gYWNjb3VudCBvciBjdXN0b2R5LgAAAAAACnJlZGVtcHRpb24AAAAAABMAAAAAAAAACHNjaGVkdWxlAAAH0AAAAAhTY2hlZHVsZQ==",
        "AAAAAgAAACNDaGFuZ2VzIHRoYXQgd2FpdCBvdXQgdGhlIHRpbWVsb2NrLgAAAAAAAAAABkNoYW5nZQAAAAAABAAAAAEAAAAAAAAACkN1cmF0b3JGZWUAAAAAAAEAAAAEAAAAAQAAAAAAAAAEVmVzdAAAAAEAAAAGAAAAAQAAAAAAAAAFUXVldWUAAAAAAAABAAAAEwAAAAEAAAA8TGlzdHMgYW4gUldBLCBvciB1cGRhdGVzIGl0cyB0ZXJtcy4gYHZhdWx0YCBtdXN0IGhvbGQgYHJ3YWAuAAAAA1J3YQAAAAADAAAAEwAAABMAAAfQAAAABVRlcm1zAAAA",
        "AAAAAQAAAEFSV0Egb3V0IHdpdGggaXRzIGlzc3VlciwgY2FycmllZCBhdCBpdHMgb3JhY2xlIHZhbHVlIHdoZW4gYm9va2VkLgAAAAAAAAAAAAAKUmVkZW1wdGlvbgAAAAAAAgAAAAAAAAAGYW1vdW50AAAAAAALAAAAAAAAAAdjYXJyaWVkAAAAAAs=",
        "AAAABAAAAAAAAAAAAAAABUVycm9yAAAAAAAADwAAAAAAAAANSW52YWxpZEFtb3VudAAAAAAAAAEAAAAAAAAADUludmFsaWRDb25maWcAAAAAAAACAAAAAAAAAA9VbnN1cHBvcnRlZFBhaXIAAAAAAwAAAAAAAAALTm90QWNjZXB0ZWQAAAAABAAAAAAAAAAHTm9QcmljZQAAAAAFAAAAAAAAAAtDYXBFeGNlZWRlZAAAAAAGAAAAAAAAABVJbnN1ZmZpY2llbnRMaXF1aWRpdHkAAAAAAAAHAAAAAAAAAAtCZWxvd01pbk91dAAAAAAIAAAAAAAAAA5Ob3RBbGxvd2xpc3RlZAAAAAAACQAAAAAAAAALVG9vTWFueVJ3YXMAAAAACgAAAAAAAAAJU3RpbGxIZWxkAAAAAAAACwAAAAAAAAAOUmVkZW1wdGlvbk9wZW4AAAAAAAwAAAAAAAAADE5vUmVkZW1wdGlvbgAAAA0AAAAAAAAADk5vdEZ1bGx5UmVwYWlkAAAAAAAOAAAAAAAAAAZQYXVzZWQAAAAAAA8=",
        "AAAAAQAAAIhBbiBhc3NldCdzIHJlZGVtcHRpb24gc2NoZWR1bGUuIFRoZSBzZXR0bGVtZW50IGNvbnRyYWN0IGtlZXBzIG9uZSBwZXIgYXNzZXQKZm9yIG1hcmtldCBtYWtlcnM7IGEgZmFjaWxpdHkga2VlcHMgaXRzIG93biBwZXIgUldBIGl0IGJ1eXMuAAAAAAAAAAhTY2hlZHVsZQAAAAUAAAAAAAAADWN5Y2xlX3NlY29uZHMAAAAAAAAEAAAAW1JhdGUgY2VpbGluZyBmb3IgdGhpcyBhc3NldCwgaW4gaHVuZHJlZHRocyBvZiBhIGJhc2lzIHBvaW50IHBlciBkYXk6CjEwMDAgaXMgMTAuMDAgYnBzL2RheS4AAAAAD21heF9icHNfcGVyX2RheQAAAAAEAAAAAAAAAARtb2RlAAAH0AAAAAxTY2hlZHVsZU1vZGUAAAAAAAAAEm5leHRfcmVkZW1wdGlvbl9hdAAAAAAABgAAAAAAAAAPcm9sbGluZ19zZWNvbmRzAAAAAAQ=",
        "AAAAAQAAAElBbiBvcmFjbGUgcmVhZGluZzogYHByaWNlYCBpcyBxdW90ZSBwZXIgYmFzZSwgMWUxOC1zY2FsZWQgb3ZlciByYXcgdW5pdHMuAAAAAAAAAAAAAAlQcmljZURhdGEAAAAAAAACAAAAAAAAAAVwcmljZQAAAAAAAAsAAAAAAAAACXRpbWVzdGFtcAAAAAAAAAY=",
        "AAAAAQAAAAAAAAAAAAAAC1F1ZXVlQ29uZmlnAAAAAAIAAAAlV2hvIGRlcGxveWVkIGl0LCB0aHJvdWdoIHRoZSBmYWN0b3J5LgAAAAAAAAdjdXJhdG9yAAAAABMAAAAAAAAACGZhY2lsaXR5AAAAEw==",
        "AAAAAQAAADZBIHZhdWx0J3Mgc2V0dGluZ3MgYXQgY3JlYXRpb24uIGBhc3NldGAgbmV2ZXIgY2hhbmdlcy4AAAAAAAAAAAALVmF1bHRDb25maWcAAAAACQAAAAAAAAAFYXNzZXQAAAAAAAATAAAAAAAAAAdjdXJhdG9yAAAAABMAAAA/UGVyZm9ybWFuY2UgZmVlIG9uIHNoYXJlLXByaWNlIGdhaW5zIGFib3ZlIHRoZSBoaWdoLXdhdGVyIG1hcmsuAAAAAA9jdXJhdG9yX2ZlZV9icHMAAAAABAAAACtOQVYgY2VpbGluZyBmb3IgZGVwb3NpdHM7IDAgbWVhbnMgdW5jYXBwZWQuAAAAAAtkZXBvc2l0X2NhcAAAAAALAAAAKU1heSByZXZva2UgYW55IHBlbmRpbmcgdGltZWxvY2tlZCBjaGFuZ2UuAAAAAAAACGd1YXJkaWFuAAAAEwAAAAAAAAAHbWFuYWdlcgAAAAATAAAAAAAAAARuYW1lAAAAEAAAAAAAAAAGc3ltYm9sAAAAAAAQAAAAOlNlY29uZHMgYSBnb3Zlcm5lZCBjaGFuZ2Ugd2FpdHMgYmVmb3JlIGl0IGNhbiBiZSBhY2NlcHRlZC4AAAAAAAh0aW1lbG9jawAAAAY=",
        "AAAAAgAAAAAAAAAAAAAADFNjaGVkdWxlTW9kZQAAAAIAAAAAAAAANUFsd2F5cyBUK046IGByb2xsaW5nX3NlY29uZHNgIHJlZ2FyZGxlc3Mgb2YgdGhlIGRhdGUuAAAAAAAAB1JvbGxpbmcAAAAAAAAAAEVSZWRlbXB0aW9uIGxhbmRzIG9uIGEgY2FsZW5kYXIgZGF0ZSBhbmQgcm9sbHMgZm9yd2FyZCBvbmNlIGl0IHBhc3Nlcy4AAAAAAAAIQ3ljbGljYWw=",
        "AAAAAQAAAI5BIEZhY2lsaXR5IGxlZydzIGBkYXRhYDogd2hpY2ggZmFjaWxpdHksIGFuZCB3aG8gcGF5cyB0aGUgaW5wdXQuIFRoZQpmYWNpbGl0eSByZXF1aXJlcyB0aGUgcGF5ZXIncyBvd24gc2lnbmF0dXJlLCBzbyBuYW1pbmcgYW55b25lIGVsc2UgZmFpbHMuAAAAAAAAAAAADUZhY2lsaXR5Um91dGUAAAAAAAACAAAAAAAAAAhmYWNpbGl0eQAAABMAAAAAAAAABXBheWVyAAAAAAAAEw==",
        "AAAAAQAAAAAAAAAAAAAADkZhY2lsaXR5Q29uZmlnAAAAAAAJAAAAPVRoZSB2YXVsdCBob2xkaW5nIHRoZSBmYWNpbGl0eSdzIGJhc2UgYXNzZXQgKGl0cyBzdGFibGVjb2luKS4AAAAAAAAKYmFzZV92YXVsdAAAAAAAEwAAAAAAAAAHY3VyYXRvcgAAAAATAAAAQ1BlcmZvcm1hbmNlIGZlZSBvbiBvTFAgc2hhcmUtcHJpY2UgZ2FpbnMgYWJvdmUgdGhlIGhpZ2gtd2F0ZXIgbWFyay4AAAAAD2N1cmF0b3JfZmVlX2JwcwAAAAAEAAAAAAAAAAhndWFyZGlhbgAAABMAAAAAAAAAB21hbmFnZXIAAAAAEwAAAAAAAAAEbmFtZQAAABAAAAAAAAAABnN5bWJvbAAAAAAAEAAAAAAAAAAIdGltZWxvY2sAAAAGAAAAQlNlY29uZHMgb3ZlciB3aGljaCBhIHRyYWRlJ3MgZ2FpbiBhZ2FpbnN0IHRoZSBvcmFjbGUgbWFyayB1bmxvY2tzLgAAAAAADHZlc3Rfc2Vjb25kcwAAAAY=",
        "AAAABAAAAAAAAAAAAAAADVRpbWVsb2NrRXJyb3IAAAAAAAADAAAAAAAAAAlOb1BlbmRpbmcAAAAAAABuAAAAAAAAAAhOb3RSZWFkeQAAAG8AAAAAAAAAC05vdEd1YXJkaWFuAAAAAHA=",
        "AAAAAQAAAAAAAAAAAAAABE1ldGEAAAADAAAAAAAAAAhkZWNpbWFscwAAAAQAAAAAAAAABG5hbWUAAAAQAAAAAAAAAAZzeW1ib2wAAAAAABA=",
        "AAAABAAAAAAAAAAAAAAAClNoYXJlRXJyb3IAAAAAAAYAAAAAAAAADUludmFsaWRBbW91bnQAAAAAAABkAAAAAAAAABNJbnN1ZmZpY2llbnRCYWxhbmNlAAAAAGUAAAAAAAAAFUluc3VmZmljaWVudEFsbG93YW5jZQAAAAAAAGYAAAAAAAAADk5vdEFsbG93bGlzdGVkAAAAAABnAAAAAAAAAAZGcm96ZW4AAAAAAGgAAAAAAAAACE92ZXJmbG93AAAAaQ==" ]),
      options
    )
  }
  public readonly fromJSON = {
    rwa: this.txFromJSON<Option<Rwa>>,
        base: this.txFromJSON<string>,
        book: this.txFromJSON<null>,
        burn: this.txFromJSON<null>,
        name: this.txFromJSON<string>,
        rwas: this.txFromJSON<Array<string>>,
        queue: this.txFromJSON<Option<string>>,
        quote: this.txFromJSON<i128>,
        trade: this.txFromJSON<i128>,
        accept: this.txFromJSON<null>,
        accrue: this.txFromJSON<null>,
        config: this.txFromJSON<FacilityConfig>,
        locked: this.txFromJSON<i128>,
        paused: this.txFromJSON<boolean>,
        redeem: this.txFromJSON<i128>,
        revoke: this.txFromJSON<null>,
        settle: this.txFromJSON<null>,
        submit: this.txFromJSON<readonly [u32, u64]>,
        symbol: this.txFromJSON<string>,
        approve: this.txFromJSON<null>,
        balance: this.txFromJSON<i128>,
        deposit: this.txFromJSON<i128>,
        pending: this.txFromJSON<Option<readonly [Change, u64]>>,
        decimals: this.txFromJSON<u32>,
        transfer: this.txFromJSON<null>,
        withdraw: this.txFromJSON<i128>,
        allowance: this.txFromJSON<i128>,
        burn_from: this.txFromJSON<null>,
        base_vault: this.txFromJSON<string>,
        redemption: this.txFromJSON<Option<Redemption>>,
        remove_rwa: this.txFromJSON<null>,
        set_member: this.txFromJSON<null>,
        set_paused: this.txFromJSON<null>,
        write_down: this.txFromJSON<null>,
        max_withdraw: this.txFromJSON<i128>,
        total_assets: this.txFromJSON<i128>,
        total_supply: this.txFromJSON<i128>,
        set_allowlist: this.txFromJSON<null>,
        transfer_from: this.txFromJSON<null>,
        convert_to_assets: this.txFromJSON<i128>,
        convert_to_shares: this.txFromJSON<i128>
  }
}