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
 * Changes that wait out the timelock. `Venue` deploys an adapter instance
 * from allowlisted code, pointed at the given venue.
 */
export type Change = {tag: "CuratorFee", values: readonly [u32]} | {tag: "DepositCap", values: readonly [i128]} | {tag: "Venue", values: readonly [Buffer, string]};

export const Errors = {
  1: {message:"InvalidAmount"},
  2: {message:"InvalidConfig"},
  3: {message:"DepositCapExceeded"},
  4: {message:"NotAllowlisted"},
  5: {message:"InsufficientLiquidity"},
  6: {message:"VenueNotFound"},
  7: {message:"VenueNotEmpty"},
  8: {message:"TooManyVenues"}
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
   * Construct and simulate a burn transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  burn: ({from, amount}: {from: string, amount: i128}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a mint transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  mint: ({shares, receiver, from, operator}: {shares: i128, receiver: string, from: string, operator: string}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a name transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  name: (options?: MethodOptions) => Promise<AssembledTransaction<string>>

  /**
   * Construct and simulate a seize transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  seize: ({from, to, amount}: {from: string, to: string, amount: i128}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a accept transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Anyone may apply a change once its delay has passed.
   */
  accept: ({kind}: {kind: u32}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a accrue transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Mints any performance fee due and moves the high-water mark.
   */
  accrue: (options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a config transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  config: (options?: MethodOptions) => Promise<AssembledTransaction<VaultConfig>>

  /**
   * Construct and simulate a freeze transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  freeze: ({who, frozen}: {who: string, frozen: boolean}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a redeem transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  redeem: ({shares, receiver, owner, operator}: {shares: i128, receiver: string, owner: string, operator: string}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a revoke transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  revoke: ({kind, caller}: {kind: u32, caller: string}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a submit transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  submit: ({change}: {change: Change}, options?: MethodOptions) => Promise<AssembledTransaction<u64>>

  /**
   * Construct and simulate a symbol transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  symbol: (options?: MethodOptions) => Promise<AssembledTransaction<string>>

  /**
   * Construct and simulate a venues transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Adapter instances, in the order exits draw on them.
   */
  venues: (options?: MethodOptions) => Promise<AssembledTransaction<Array<string>>>

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
   * Construct and simulate a upgrade transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Protocol admin only, and only to the code the host has registered.
   */
  upgrade: ({new_wasm_hash}: {new_wasm_hash: Buffer}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a allocate transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  allocate: ({adapter, amount}: {adapter: string, amount: i128}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a decimals transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  decimals: (options?: MethodOptions) => Promise<AssembledTransaction<u32>>

  /**
   * Construct and simulate a max_mint transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  max_mint: ({receiver}: {receiver: string}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

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
   * Construct and simulate a deallocate transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns what actually came back, measured here.
   */
  deallocate: ({adapter, amount}: {adapter: string, amount: i128}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a max_redeem transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  max_redeem: ({owner}: {owner: string}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a set_member transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_member: ({who, allowed}: {who: string, allowed: boolean}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a max_deposit transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  max_deposit: ({receiver}: {receiver: string}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a query_asset transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  query_asset: (options?: MethodOptions) => Promise<AssembledTransaction<string>>

  /**
   * Construct and simulate a set_manager transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_manager: ({manager}: {manager: string}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a max_withdraw transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  max_withdraw: ({owner}: {owner: string}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a preview_mint transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  preview_mint: ({shares}: {shares: i128}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a total_assets transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  total_assets: (options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a total_supply transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  total_supply: (options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a disable_venue transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Only an empty venue can be dropped, so no position is ever orphaned.
   */
  disable_venue: ({adapter}: {adapter: string}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a set_allowlist transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_allowlist: ({enabled}: {enabled: boolean}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a transfer_from transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  transfer_from: ({spender, from, to, amount}: {spender: string, from: string, to: string, amount: i128}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a preview_redeem transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  preview_redeem: ({shares}: {shares: i128}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a high_water_mark transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  high_water_mark: (options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a preview_deposit transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  preview_deposit: ({assets}: {assets: i128}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a set_venue_order transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * `order` must name exactly the current set.
   */
  set_venue_order: ({order}: {order: Array<string>}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a preview_withdraw transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  preview_withdraw: ({assets}: {assets: i128}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

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
        {host, config}: {host: string, config: VaultConfig},
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
      new ContractSpec([ "AAAAAAAAAAAAAAAEYnVybgAAAAIAAAAAAAAABGZyb20AAAATAAAAAAAAAAZhbW91bnQAAAAAAAsAAAAA",
        "AAAAAAAAAAAAAAAEbWludAAAAAQAAAAAAAAABnNoYXJlcwAAAAAACwAAAAAAAAAIcmVjZWl2ZXIAAAATAAAAAAAAAARmcm9tAAAAEwAAAAAAAAAIb3BlcmF0b3IAAAATAAAAAQAAAAs=",
        "AAAAAAAAAAAAAAAEbmFtZQAAAAAAAAABAAAAEA==",
        "AAAAAAAAAAAAAAAFc2VpemUAAAAAAAADAAAAAAAAAARmcm9tAAAAEwAAAAAAAAACdG8AAAAAABMAAAAAAAAABmFtb3VudAAAAAAACwAAAAA=",
        "AAAAAAAAADRBbnlvbmUgbWF5IGFwcGx5IGEgY2hhbmdlIG9uY2UgaXRzIGRlbGF5IGhhcyBwYXNzZWQuAAAABmFjY2VwdAAAAAAAAQAAAAAAAAAEa2luZAAAAAQAAAAA",
        "AAAAAAAAADxNaW50cyBhbnkgcGVyZm9ybWFuY2UgZmVlIGR1ZSBhbmQgbW92ZXMgdGhlIGhpZ2gtd2F0ZXIgbWFyay4AAAAGYWNjcnVlAAAAAAAAAAAAAA==",
        "AAAAAAAAAAAAAAAGY29uZmlnAAAAAAAAAAAAAQAAB9AAAAALVmF1bHRDb25maWcA",
        "AAAAAAAAAAAAAAAGZnJlZXplAAAAAAACAAAAAAAAAAN3aG8AAAAAEwAAAAAAAAAGZnJvemVuAAAAAAABAAAAAA==",
        "AAAAAAAAAAAAAAAGcmVkZWVtAAAAAAAEAAAAAAAAAAZzaGFyZXMAAAAAAAsAAAAAAAAACHJlY2VpdmVyAAAAEwAAAAAAAAAFb3duZXIAAAAAAAATAAAAAAAAAAhvcGVyYXRvcgAAABMAAAABAAAACw==",
        "AAAAAAAAAAAAAAAGcmV2b2tlAAAAAAACAAAAAAAAAARraW5kAAAABAAAAAAAAAAGY2FsbGVyAAAAAAATAAAAAA==",
        "AAAAAAAAAAAAAAAGc3VibWl0AAAAAAABAAAAAAAAAAZjaGFuZ2UAAAAAB9AAAAAGQ2hhbmdlAAAAAAABAAAABg==",
        "AAAAAAAAAAAAAAAGc3ltYm9sAAAAAAAAAAAAAQAAABA=",
        "AAAAAAAAADNBZGFwdGVyIGluc3RhbmNlcywgaW4gdGhlIG9yZGVyIGV4aXRzIGRyYXcgb24gdGhlbS4AAAAABnZlbnVlcwAAAAAAAAAAAAEAAAPqAAAAEw==",
        "AAAAAAAAAAAAAAAHYXBwcm92ZQAAAAAEAAAAAAAAAARmcm9tAAAAEwAAAAAAAAAHc3BlbmRlcgAAAAATAAAAAAAAAAZhbW91bnQAAAAAAAsAAAAAAAAAEWV4cGlyYXRpb25fbGVkZ2VyAAAAAAAABAAAAAA=",
        "AAAAAAAAAAAAAAAHYmFsYW5jZQAAAAABAAAAAAAAAAJpZAAAAAAAEwAAAAEAAAAL",
        "AAAAAAAAAAAAAAAHZGVwb3NpdAAAAAAEAAAAAAAAAAZhc3NldHMAAAAAAAsAAAAAAAAACHJlY2VpdmVyAAAAEwAAAAAAAAAEZnJvbQAAABMAAAAAAAAACG9wZXJhdG9yAAAAEwAAAAEAAAAL",
        "AAAAAAAAAAAAAAAHcGVuZGluZwAAAAABAAAAAAAAAARraW5kAAAABAAAAAEAAAPoAAAD7QAAAAIAAAfQAAAABkNoYW5nZQAAAAAABg==",
        "AAAAAAAAAEJQcm90b2NvbCBhZG1pbiBvbmx5LCBhbmQgb25seSB0byB0aGUgY29kZSB0aGUgaG9zdCBoYXMgcmVnaXN0ZXJlZC4AAAAAAAd1cGdyYWRlAAAAAAEAAAAAAAAADW5ld193YXNtX2hhc2gAAAAAAAPuAAAAIAAAAAA=",
        "AAAAAAAAAAAAAAAIYWxsb2NhdGUAAAACAAAAAAAAAAdhZGFwdGVyAAAAABMAAAAAAAAABmFtb3VudAAAAAAACwAAAAA=",
        "AAAAAAAAAAAAAAAIZGVjaW1hbHMAAAAAAAAAAQAAAAQ=",
        "AAAAAAAAAAAAAAAIbWF4X21pbnQAAAABAAAAAAAAAAhyZWNlaXZlcgAAABMAAAABAAAACw==",
        "AAAAAAAAAAAAAAAIdHJhbnNmZXIAAAADAAAAAAAAAARmcm9tAAAAEwAAAAAAAAACdG8AAAAAABMAAAAAAAAABmFtb3VudAAAAAAACwAAAAA=",
        "AAAAAAAAAAAAAAAId2l0aGRyYXcAAAAEAAAAAAAAAAZhc3NldHMAAAAAAAsAAAAAAAAACHJlY2VpdmVyAAAAEwAAAAAAAAAFb3duZXIAAAAAAAATAAAAAAAAAAhvcGVyYXRvcgAAABMAAAABAAAACw==",
        "AAAAAgAAAHpDaGFuZ2VzIHRoYXQgd2FpdCBvdXQgdGhlIHRpbWVsb2NrLiBgVmVudWVgIGRlcGxveXMgYW4gYWRhcHRlciBpbnN0YW5jZQpmcm9tIGFsbG93bGlzdGVkIGNvZGUsIHBvaW50ZWQgYXQgdGhlIGdpdmVuIHZlbnVlLgAAAAAAAAAAAAZDaGFuZ2UAAAAAAAMAAAABAAAAAAAAAApDdXJhdG9yRmVlAAAAAAABAAAABAAAAAEAAAAAAAAACkRlcG9zaXRDYXAAAAAAAAEAAAALAAAAAQAAAAAAAAAFVmVudWUAAAAAAAACAAAD7gAAACAAAAAT",
        "AAAAAAAAAAAAAAAJYWxsb3dhbmNlAAAAAAAAAgAAAAAAAAAEZnJvbQAAABMAAAAAAAAAB3NwZW5kZXIAAAAAEwAAAAEAAAAL",
        "AAAAAAAAAAAAAAAJYnVybl9mcm9tAAAAAAAAAwAAAAAAAAAHc3BlbmRlcgAAAAATAAAAAAAAAARmcm9tAAAAEwAAAAAAAAAGYW1vdW50AAAAAAALAAAAAA==",
        "AAAAAAAAAC9SZXR1cm5zIHdoYXQgYWN0dWFsbHkgY2FtZSBiYWNrLCBtZWFzdXJlZCBoZXJlLgAAAAAKZGVhbGxvY2F0ZQAAAAAAAgAAAAAAAAAHYWRhcHRlcgAAAAATAAAAAAAAAAZhbW91bnQAAAAAAAsAAAABAAAACw==",
        "AAAAAAAAAAAAAAAKbWF4X3JlZGVlbQAAAAAAAQAAAAAAAAAFb3duZXIAAAAAAAATAAAAAQAAAAs=",
        "AAAAAAAAAAAAAAAKc2V0X21lbWJlcgAAAAAAAgAAAAAAAAADd2hvAAAAABMAAAAAAAAAB2FsbG93ZWQAAAAAAQAAAAA=",
        "AAAAAAAAAAAAAAALbWF4X2RlcG9zaXQAAAAAAQAAAAAAAAAIcmVjZWl2ZXIAAAATAAAAAQAAAAs=",
        "AAAAAAAAAAAAAAALcXVlcnlfYXNzZXQAAAAAAAAAAAEAAAAT",
        "AAAAAAAAAAAAAAALc2V0X21hbmFnZXIAAAAAAQAAAAAAAAAHbWFuYWdlcgAAAAATAAAAAA==",
        "AAAAAAAAAAAAAAAMbWF4X3dpdGhkcmF3AAAAAQAAAAAAAAAFb3duZXIAAAAAAAATAAAAAQAAAAs=",
        "AAAAAAAAAAAAAAAMcHJldmlld19taW50AAAAAQAAAAAAAAAGc2hhcmVzAAAAAAALAAAAAQAAAAs=",
        "AAAAAAAAAAAAAAAMdG90YWxfYXNzZXRzAAAAAAAAAAEAAAAL",
        "AAAAAAAAAAAAAAAMdG90YWxfc3VwcGx5AAAAAAAAAAEAAAAL",
        "AAAAAAAAAERPbmx5IGFuIGVtcHR5IHZlbnVlIGNhbiBiZSBkcm9wcGVkLCBzbyBubyBwb3NpdGlvbiBpcyBldmVyIG9ycGhhbmVkLgAAAA1kaXNhYmxlX3ZlbnVlAAAAAAAAAQAAAAAAAAAHYWRhcHRlcgAAAAATAAAAAA==",
        "AAAAAAAAAAAAAAANc2V0X2FsbG93bGlzdAAAAAAAAAEAAAAAAAAAB2VuYWJsZWQAAAAAAQAAAAA=",
        "AAAAAAAAAAAAAAANdHJhbnNmZXJfZnJvbQAAAAAAAAQAAAAAAAAAB3NwZW5kZXIAAAAAEwAAAAAAAAAEZnJvbQAAABMAAAAAAAAAAnRvAAAAAAATAAAAAAAAAAZhbW91bnQAAAAAAAsAAAAA",
        "AAAAAAAAAAAAAAANX19jb25zdHJ1Y3RvcgAAAAAAAAIAAAAAAAAABGhvc3QAAAATAAAAAAAAAAZjb25maWcAAAAAB9AAAAALVmF1bHRDb25maWcAAAAAAA==",
        "AAAAAAAAAAAAAAAOcHJldmlld19yZWRlZW0AAAAAAAEAAAAAAAAABnNoYXJlcwAAAAAACwAAAAEAAAAL",
        "AAAAAAAAAAAAAAAPaGlnaF93YXRlcl9tYXJrAAAAAAAAAAABAAAACw==",
        "AAAAAAAAAAAAAAAPcHJldmlld19kZXBvc2l0AAAAAAEAAAAAAAAABmFzc2V0cwAAAAAACwAAAAEAAAAL",
        "AAAAAAAAACpgb3JkZXJgIG11c3QgbmFtZSBleGFjdGx5IHRoZSBjdXJyZW50IHNldC4AAAAAAA9zZXRfdmVudWVfb3JkZXIAAAAAAQAAAAAAAAAFb3JkZXIAAAAAAAPqAAAAEwAAAAA=",
        "AAAAAAAAAAAAAAAQcHJldmlld193aXRoZHJhdwAAAAEAAAAAAAAABmFzc2V0cwAAAAAACwAAAAEAAAAL",
        "AAAAAAAAAAAAAAARY29udmVydF90b19hc3NldHMAAAAAAAABAAAAAAAAAAZzaGFyZXMAAAAAAAsAAAABAAAACw==",
        "AAAAAAAAAAAAAAARY29udmVydF90b19zaGFyZXMAAAAAAAABAAAAAAAAAAZhc3NldHMAAAAAAAsAAAABAAAACw==",
        "AAAABAAAAAAAAAAAAAAABUVycm9yAAAAAAAACAAAAAAAAAANSW52YWxpZEFtb3VudAAAAAAAAAEAAAAAAAAADUludmFsaWRDb25maWcAAAAAAAACAAAAAAAAABJEZXBvc2l0Q2FwRXhjZWVkZWQAAAAAAAMAAAAAAAAADk5vdEFsbG93bGlzdGVkAAAAAAAEAAAAAAAAABVJbnN1ZmZpY2llbnRMaXF1aWRpdHkAAAAAAAAFAAAAAAAAAA1WZW51ZU5vdEZvdW5kAAAAAAAABgAAAAAAAAANVmVudWVOb3RFbXB0eQAAAAAAAAcAAAAAAAAADVRvb01hbnlWZW51ZXMAAAAAAAAI",
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
    burn: this.txFromJSON<null>,
        mint: this.txFromJSON<i128>,
        name: this.txFromJSON<string>,
        seize: this.txFromJSON<null>,
        accept: this.txFromJSON<null>,
        accrue: this.txFromJSON<null>,
        config: this.txFromJSON<VaultConfig>,
        freeze: this.txFromJSON<null>,
        redeem: this.txFromJSON<i128>,
        revoke: this.txFromJSON<null>,
        submit: this.txFromJSON<u64>,
        symbol: this.txFromJSON<string>,
        venues: this.txFromJSON<Array<string>>,
        approve: this.txFromJSON<null>,
        balance: this.txFromJSON<i128>,
        deposit: this.txFromJSON<i128>,
        pending: this.txFromJSON<Option<readonly [Change, u64]>>,
        upgrade: this.txFromJSON<null>,
        allocate: this.txFromJSON<null>,
        decimals: this.txFromJSON<u32>,
        max_mint: this.txFromJSON<i128>,
        transfer: this.txFromJSON<null>,
        withdraw: this.txFromJSON<i128>,
        allowance: this.txFromJSON<i128>,
        burn_from: this.txFromJSON<null>,
        deallocate: this.txFromJSON<i128>,
        max_redeem: this.txFromJSON<i128>,
        set_member: this.txFromJSON<null>,
        max_deposit: this.txFromJSON<i128>,
        query_asset: this.txFromJSON<string>,
        set_manager: this.txFromJSON<null>,
        max_withdraw: this.txFromJSON<i128>,
        preview_mint: this.txFromJSON<i128>,
        total_assets: this.txFromJSON<i128>,
        total_supply: this.txFromJSON<i128>,
        disable_venue: this.txFromJSON<null>,
        set_allowlist: this.txFromJSON<null>,
        transfer_from: this.txFromJSON<null>,
        preview_redeem: this.txFromJSON<i128>,
        high_water_mark: this.txFromJSON<i128>,
        preview_deposit: this.txFromJSON<i128>,
        set_venue_order: this.txFromJSON<null>,
        preview_withdraw: this.txFromJSON<i128>,
        convert_to_assets: this.txFromJSON<i128>,
        convert_to_shares: this.txFromJSON<i128>
  }
}