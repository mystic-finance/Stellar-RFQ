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






export interface Quote {
  fee: i128;
  horizon_seconds: u32;
  maker_amount: i128;
}


export interface Config {
  decay_seconds: u32;
  fallback_max_age: u64;
  max_deviation_bps: u32;
  max_fee_bps: u32;
  max_shift_seconds: u32;
  min_fee_bps: u32;
  /**
 * Seconds a keeper must wait between backstop pushes. Floors at one ledger.
 */
min_push_interval: u64;
}


export interface Listing {
  active: boolean;
  created_at: u64;
  decay_seconds: u32;
  order: DutchOrder;
  seller: string;
}


export interface OracleCfg {
  max_age: u64;
  oracle: string;
}


export interface Reference {
  asset: string;
  epoch: u32;
}


export interface DutchOrder {
  expiry: u64;
  fee_bps: u32;
  fee_recipient: string;
  maker_token: string;
  min_maker_amount: i128;
  start_maker_amount: i128;
  taker_amount: i128;
  taker_token: string;
}


export interface PushedPrice {
  epoch: u32;
  price: i128;
  updated_at: u64;
}

export const Errors = {
  1: {message:"AlreadyInitialized"},
  2: {message:"NotAuthorized"},
  3: {message:"Paused"},
  4: {message:"OrderNotFillable"},
  5: {message:"NotFillableByTaker"},
  6: {message:"SignerNotAuthorized"},
  7: {message:"InvalidAmount"},
  8: {message:"Overflow"},
  9: {message:"NoSchedule"},
  10: {message:"InvalidSchedule"},
  11: {message:"NoPrice"},
  12: {message:"BpsPerDayTooHigh"},
  13: {message:"DiscountTooLarge"},
  14: {message:"FeeOutOfBounds"},
  15: {message:"BelowMinReceived"},
  16: {message:"MakerAmountTooHigh"},
  17: {message:"SameToken"},
  18: {message:"ListingNotActive"},
  19: {message:"AskAboveMax"},
  20: {message:"PriceDeviation"},
  21: {message:"ScheduleShiftTooLarge"},
  22: {message:"InvalidConfig"},
  23: {message:"WrongSender"},
  24: {message:"SaltIsCancelled"},
  25: {message:"RequestOverfilled"},
  26: {message:"PriceUpdateTooSoon"}
}

export type DataKey = {tag: "Admin", values: void} | {tag: "Cfg", values: void} | {tag: "Ref", values: void} | {tag: "Paused", values: void} | {tag: "NextId", values: void} | {tag: "Keeper", values: readonly [string]} | {tag: "Sched", values: readonly [string]} | {tag: "Oracle", values: readonly [string, string]} | {tag: "Fallback", values: readonly [string]} | {tag: "Filled", values: readonly [Buffer]} | {tag: "RequestFilled", values: readonly [Buffer]} | {tag: "SaltCancelled", values: readonly [string, u64]} | {tag: "Signer", values: readonly [string, Buffer]} | {tag: "Listing", values: readonly [u64]};


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


export interface Request {
  expiry: u64;
  fee_bps: u32;
  fee_recipient: string;
  maker_token: string;
  min_received_amount: i128;
  order_type: OrderType;
  salt: u64;
  sender: Option<string>;
  taker: Option<string>;
  taker_amount: i128;
  /**
 * Ceiling on the maker's rate, in hundredths of a basis point per day:
 * 250 is 2.50 bps/day.
 */
taker_max_bps_per_day: u32;
  taker_token: string;
}


export interface RfqOrder {
  expiry: u64;
  fee_bps: u32;
  fee_recipient: string;
  maker: string;
  /**
 * The bid, in hundredths of a basis point per day: 250 is 2.50 bps/day.
 */
maker_bps_per_day: u32;
  maker_token: string;
  max_maker_amount: i128;
  min_received_amount: i128;
  salt: u64;
  sender: Option<string>;
  taker: Option<string>;
  taker_amount: i128;
  /**
 * Ceiling on the maker's rate, in hundredths of a basis point per day.
 */
taker_max_bps_per_day: u32;
  taker_token: string;
}

export type OrderType = {tag: "Rfq", values: void} | {tag: "Fixed", values: void};


export interface Signature {
  signature: Buffer;
  signer: Buffer;
}


export interface FillResult {
  fee: i128;
  maker_filled: i128;
  taker_filled: i128;
}


export interface FixedOrder {
  expiry: u64;
  fee_bps: u32;
  fee_recipient: string;
  maker: string;
  maker_amount: i128;
  maker_token: string;
  min_received_amount: i128;
  salt: u64;
  sender: Option<string>;
  taker: Option<string>;
  taker_amount: i128;
  taker_token: string;
}

export interface Client {
  /**
   * Construct and simulate a upgrade transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  upgrade: ({new_wasm_hash}: {new_wasm_hash: Buffer}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a price_of transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  price_of: ({base, quote}: {base: string, quote: string}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a get_admin transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_admin: (options?: MethodOptions) => Promise<AssembledTransaction<string>>

  /**
   * Construct and simulate a is_paused transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  is_paused: (options?: MethodOptions) => Promise<AssembledTransaction<boolean>>

  /**
   * Construct and simulate a get_config transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_config: (options?: MethodOptions) => Promise<AssembledTransaction<Config>>

  /**
   * Construct and simulate a initialize transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  initialize: ({admin, reference}: {admin: string, reference: string}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a push_price transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  push_price: ({caller, asset, new_price}: {caller: string, asset: string, new_price: i128}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a set_config transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_config: ({cfg}: {cfg: Config}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a set_keeper transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_keeper: ({keeper, allowed}: {keeper: string, allowed: boolean}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a set_oracle transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_oracle: ({base, quote, cfg}: {base: string, quote: string, cfg: Option<OracleCfg>}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a set_paused transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_paused: ({paused}: {paused: boolean}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a cancel_salt transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  cancel_salt: ({caller, signer, salt}: {caller: string, signer: string, salt: u64}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a current_ask transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  current_ask: ({id}: {id: u64}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a get_listing transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_listing: ({id}: {id: u64}, options?: MethodOptions) => Promise<AssembledTransaction<Option<Listing>>>

  /**
   * Construct and simulate a get_schedule transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_schedule: ({asset}: {asset: string}, options?: MethodOptions) => Promise<AssembledTransaction<Option<Schedule>>>

  /**
   * Construct and simulate a hash_request transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  hash_request: ({request}: {request: Request}, options?: MethodOptions) => Promise<AssembledTransaction<Buffer>>

  /**
   * Construct and simulate a set_schedule transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_schedule: ({caller, asset, schedule}: {caller: string, asset: string, schedule: Schedule}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a filled_amount transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  filled_amount: ({order_hash}: {order_hash: Buffer}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a get_reference transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_reference: (options?: MethodOptions) => Promise<AssembledTransaction<Reference>>

  /**
   * Construct and simulate a set_reference transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_reference: ({asset}: {asset: string}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a fill_rfq_order transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  fill_rfq_order: ({order, maker_signature, taker_signature, sender, taker_amount_in}: {order: RfqOrder, maker_signature: Signature, taker_signature: Option<Signature>, sender: string, taker_amount_in: i128}, options?: MethodOptions) => Promise<AssembledTransaction<FillResult>>

  /**
   * Construct and simulate a hash_rfq_order transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  hash_rfq_order: ({order}: {order: RfqOrder}, options?: MethodOptions) => Promise<AssembledTransaction<Buffer>>

  /**
   * Construct and simulate a is_order_signer transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  is_order_signer: ({maker, signer}: {maker: string, signer: Buffer}, options?: MethodOptions) => Promise<AssembledTransaction<boolean>>

  /**
   * Construct and simulate a quote_rfq_order transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  quote_rfq_order: ({order, taker_amount_in}: {order: RfqOrder, taker_amount_in: i128}, options?: MethodOptions) => Promise<AssembledTransaction<Quote>>

  /**
   * Construct and simulate a fill_dutch_order transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  fill_dutch_order: ({id, buyer, max_maker_amount}: {id: u64, buyer: string, max_maker_amount: i128}, options?: MethodOptions) => Promise<AssembledTransaction<FillResult>>

  /**
   * Construct and simulate a fill_fixed_order transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  fill_fixed_order: ({order, maker_signature, taker_signature, sender, taker_amount_in}: {order: FixedOrder, maker_signature: Signature, taker_signature: Option<Signature>, sender: string, taker_amount_in: i128}, options?: MethodOptions) => Promise<AssembledTransaction<FillResult>>

  /**
   * Construct and simulate a hash_fixed_order transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  hash_fixed_order: ({order}: {order: FixedOrder}, options?: MethodOptions) => Promise<AssembledTransaction<Buffer>>

  /**
   * Construct and simulate a is_salt_cancelled transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  is_salt_cancelled: ({signer, salt}: {signer: string, salt: u64}, options?: MethodOptions) => Promise<AssembledTransaction<boolean>>

  /**
   * Construct and simulate a cancel_dutch_order transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  cancel_dutch_order: ({id}: {id: u64}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a create_dutch_order transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  create_dutch_order: ({seller, order}: {seller: string, order: DutchOrder}, options?: MethodOptions) => Promise<AssembledTransaction<u64>>

  /**
   * Construct and simulate a register_order_signer transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  register_order_signer: ({maker, signer, allowed}: {maker: string, signer: Buffer, allowed: boolean}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a request_filled_amount transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  request_filled_amount: ({request_hash}: {request_hash: Buffer}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a seconds_to_redemption transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  seconds_to_redemption: ({asset}: {asset: string}, options?: MethodOptions) => Promise<AssembledTransaction<u32>>

}
export class Client extends ContractClient {
  static async deploy<T = Client>(
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
    return ContractClient.deploy(null, options)
  }
  constructor(public readonly options: ContractClientOptions) {
    super(
      new ContractSpec([ "AAAAAAAAAAAAAAAHdXBncmFkZQAAAAABAAAAAAAAAA1uZXdfd2FzbV9oYXNoAAAAAAAD7gAAACAAAAAA",
        "AAAAAAAAAAAAAAAIcHJpY2Vfb2YAAAACAAAAAAAAAARiYXNlAAAAEwAAAAAAAAAFcXVvdGUAAAAAAAATAAAAAQAAAAs=",
        "AAAAAAAAAAAAAAAJZ2V0X2FkbWluAAAAAAAAAAAAAAEAAAAT",
        "AAAAAAAAAAAAAAAJaXNfcGF1c2VkAAAAAAAAAAAAAAEAAAAB",
        "AAAAAAAAAAAAAAAKZ2V0X2NvbmZpZwAAAAAAAAAAAAEAAAfQAAAABkNvbmZpZwAA",
        "AAAAAAAAAAAAAAAKaW5pdGlhbGl6ZQAAAAAAAgAAAAAAAAAFYWRtaW4AAAAAAAATAAAAAAAAAAlyZWZlcmVuY2UAAAAAAAATAAAAAA==",
        "AAAAAAAAAAAAAAAKcHVzaF9wcmljZQAAAAAAAwAAAAAAAAAGY2FsbGVyAAAAAAATAAAAAAAAAAVhc3NldAAAAAAAABMAAAAAAAAACW5ld19wcmljZQAAAAAAAAsAAAAA",
        "AAAAAAAAAAAAAAAKc2V0X2NvbmZpZwAAAAAAAQAAAAAAAAADY2ZnAAAAB9AAAAAGQ29uZmlnAAAAAAAA",
        "AAAAAAAAAAAAAAAKc2V0X2tlZXBlcgAAAAAAAgAAAAAAAAAGa2VlcGVyAAAAAAATAAAAAAAAAAdhbGxvd2VkAAAAAAEAAAAA",
        "AAAAAAAAAAAAAAAKc2V0X29yYWNsZQAAAAAAAwAAAAAAAAAEYmFzZQAAABMAAAAAAAAABXF1b3RlAAAAAAAAEwAAAAAAAAADY2ZnAAAAA+gAAAfQAAAACU9yYWNsZUNmZwAAAAAAAAA=",
        "AAAAAAAAAAAAAAAKc2V0X3BhdXNlZAAAAAAAAQAAAAAAAAAGcGF1c2VkAAAAAAABAAAAAA==",
        "AAAAAAAAAAAAAAALY2FuY2VsX3NhbHQAAAAAAwAAAAAAAAAGY2FsbGVyAAAAAAATAAAAAAAAAAZzaWduZXIAAAAAABMAAAAAAAAABHNhbHQAAAAGAAAAAA==",
        "AAAAAAAAAAAAAAALY3VycmVudF9hc2sAAAAAAQAAAAAAAAACaWQAAAAAAAYAAAABAAAACw==",
        "AAAAAAAAAAAAAAALZ2V0X2xpc3RpbmcAAAAAAQAAAAAAAAACaWQAAAAAAAYAAAABAAAD6AAAB9AAAAAHTGlzdGluZwA=",
        "AAAAAAAAAAAAAAAMZ2V0X3NjaGVkdWxlAAAAAQAAAAAAAAAFYXNzZXQAAAAAAAATAAAAAQAAA+gAAAfQAAAACFNjaGVkdWxl",
        "AAAAAAAAAAAAAAAMaGFzaF9yZXF1ZXN0AAAAAQAAAAAAAAAHcmVxdWVzdAAAAAfQAAAAB1JlcXVlc3QAAAAAAQAAA+4AAAAg",
        "AAAAAAAAAAAAAAAMc2V0X3NjaGVkdWxlAAAAAwAAAAAAAAAGY2FsbGVyAAAAAAATAAAAAAAAAAVhc3NldAAAAAAAABMAAAAAAAAACHNjaGVkdWxlAAAH0AAAAAhTY2hlZHVsZQAAAAA=",
        "AAAAAAAAAAAAAAANZmlsbGVkX2Ftb3VudAAAAAAAAAEAAAAAAAAACm9yZGVyX2hhc2gAAAAAA+4AAAAgAAAAAQAAAAs=",
        "AAAAAAAAAAAAAAANZ2V0X3JlZmVyZW5jZQAAAAAAAAAAAAABAAAH0AAAAAlSZWZlcmVuY2UAAAA=",
        "AAAAAAAAAAAAAAANc2V0X3JlZmVyZW5jZQAAAAAAAAEAAAAAAAAABWFzc2V0AAAAAAAAEwAAAAA=",
        "AAAAAAAAAAAAAAAOZmlsbF9yZnFfb3JkZXIAAAAAAAUAAAAAAAAABW9yZGVyAAAAAAAH0AAAAAhSZnFPcmRlcgAAAAAAAAAPbWFrZXJfc2lnbmF0dXJlAAAAB9AAAAAJU2lnbmF0dXJlAAAAAAAAAAAAAA90YWtlcl9zaWduYXR1cmUAAAAD6AAAB9AAAAAJU2lnbmF0dXJlAAAAAAAAAAAAAAZzZW5kZXIAAAAAABMAAAAAAAAAD3Rha2VyX2Ftb3VudF9pbgAAAAALAAAAAQAAB9AAAAAKRmlsbFJlc3VsdAAA",
        "AAAAAAAAAAAAAAAOaGFzaF9yZnFfb3JkZXIAAAAAAAEAAAAAAAAABW9yZGVyAAAAAAAH0AAAAAhSZnFPcmRlcgAAAAEAAAPuAAAAIA==",
        "AAAAAAAAAAAAAAAPaXNfb3JkZXJfc2lnbmVyAAAAAAIAAAAAAAAABW1ha2VyAAAAAAAAEwAAAAAAAAAGc2lnbmVyAAAAAAPuAAAAIAAAAAEAAAAB",
        "AAAAAAAAAAAAAAAPcXVvdGVfcmZxX29yZGVyAAAAAAIAAAAAAAAABW9yZGVyAAAAAAAH0AAAAAhSZnFPcmRlcgAAAAAAAAAPdGFrZXJfYW1vdW50X2luAAAAAAsAAAABAAAH0AAAAAVRdW90ZQAAAA==",
        "AAAAAAAAAAAAAAAQZmlsbF9kdXRjaF9vcmRlcgAAAAMAAAAAAAAAAmlkAAAAAAAGAAAAAAAAAAVidXllcgAAAAAAABMAAAAAAAAAEG1heF9tYWtlcl9hbW91bnQAAAALAAAAAQAAB9AAAAAKRmlsbFJlc3VsdAAA",
        "AAAAAAAAAAAAAAAQZmlsbF9maXhlZF9vcmRlcgAAAAUAAAAAAAAABW9yZGVyAAAAAAAH0AAAAApGaXhlZE9yZGVyAAAAAAAAAAAAD21ha2VyX3NpZ25hdHVyZQAAAAfQAAAACVNpZ25hdHVyZQAAAAAAAAAAAAAPdGFrZXJfc2lnbmF0dXJlAAAAA+gAAAfQAAAACVNpZ25hdHVyZQAAAAAAAAAAAAAGc2VuZGVyAAAAAAATAAAAAAAAAA90YWtlcl9hbW91bnRfaW4AAAAACwAAAAEAAAfQAAAACkZpbGxSZXN1bHQAAA==",
        "AAAAAAAAAAAAAAAQaGFzaF9maXhlZF9vcmRlcgAAAAEAAAAAAAAABW9yZGVyAAAAAAAH0AAAAApGaXhlZE9yZGVyAAAAAAABAAAD7gAAACA=",
        "AAAAAAAAAAAAAAARaXNfc2FsdF9jYW5jZWxsZWQAAAAAAAACAAAAAAAAAAZzaWduZXIAAAAAABMAAAAAAAAABHNhbHQAAAAGAAAAAQAAAAE=",
        "AAAAAAAAAAAAAAASY2FuY2VsX2R1dGNoX29yZGVyAAAAAAABAAAAAAAAAAJpZAAAAAAABgAAAAA=",
        "AAAAAAAAAAAAAAASY3JlYXRlX2R1dGNoX29yZGVyAAAAAAACAAAAAAAAAAZzZWxsZXIAAAAAABMAAAAAAAAABW9yZGVyAAAAAAAH0AAAAApEdXRjaE9yZGVyAAAAAAABAAAABg==",
        "AAAAAAAAAAAAAAAVcmVnaXN0ZXJfb3JkZXJfc2lnbmVyAAAAAAAAAwAAAAAAAAAFbWFrZXIAAAAAAAATAAAAAAAAAAZzaWduZXIAAAAAA+4AAAAgAAAAAAAAAAdhbGxvd2VkAAAAAAEAAAAA",
        "AAAAAAAAAAAAAAAVcmVxdWVzdF9maWxsZWRfYW1vdW50AAAAAAAAAQAAAAAAAAAMcmVxdWVzdF9oYXNoAAAD7gAAACAAAAABAAAACw==",
        "AAAAAAAAAAAAAAAVc2Vjb25kc190b19yZWRlbXB0aW9uAAAAAAAAAQAAAAAAAAAFYXNzZXQAAAAAAAATAAAAAQAAAAQ=",
        "AAAAAQAAAAAAAAAAAAAABVF1b3RlAAAAAAAAAwAAAAAAAAADZmVlAAAAAAsAAAAAAAAAD2hvcml6b25fc2Vjb25kcwAAAAAEAAAAAAAAAAxtYWtlcl9hbW91bnQAAAAL",
        "AAAAAQAAAAAAAAAAAAAABkNvbmZpZwAAAAAABwAAAAAAAAANZGVjYXlfc2Vjb25kcwAAAAAAAAQAAAAAAAAAEGZhbGxiYWNrX21heF9hZ2UAAAAGAAAAAAAAABFtYXhfZGV2aWF0aW9uX2JwcwAAAAAAAAQAAAAAAAAAC21heF9mZWVfYnBzAAAAAAQAAAAAAAAAEW1heF9zaGlmdF9zZWNvbmRzAAAAAAAABAAAAAAAAAALbWluX2ZlZV9icHMAAAAABAAAAElTZWNvbmRzIGEga2VlcGVyIG11c3Qgd2FpdCBiZXR3ZWVuIGJhY2tzdG9wIHB1c2hlcy4gRmxvb3JzIGF0IG9uZSBsZWRnZXIuAAAAAAAAEW1pbl9wdXNoX2ludGVydmFsAAAAAAAABg==",
        "AAAAAQAAAAAAAAAAAAAAB0xpc3RpbmcAAAAABQAAAAAAAAAGYWN0aXZlAAAAAAABAAAAAAAAAApjcmVhdGVkX2F0AAAAAAAGAAAAAAAAAA1kZWNheV9zZWNvbmRzAAAAAAAABAAAAAAAAAAFb3JkZXIAAAAAAAfQAAAACkR1dGNoT3JkZXIAAAAAAAAAAAAGc2VsbGVyAAAAAAAT",
        "AAAAAQAAAAAAAAAAAAAACU9yYWNsZUNmZwAAAAAAAAIAAAAAAAAAB21heF9hZ2UAAAAABgAAAAAAAAAGb3JhY2xlAAAAAAAT",
        "AAAAAQAAAAAAAAAAAAAACVJlZmVyZW5jZQAAAAAAAAIAAAAAAAAABWFzc2V0AAAAAAAAEwAAAAAAAAAFZXBvY2gAAAAAAAAE",
        "AAAAAQAAAAAAAAAAAAAACkR1dGNoT3JkZXIAAAAAAAgAAAAAAAAABmV4cGlyeQAAAAAABgAAAAAAAAAHZmVlX2JwcwAAAAAEAAAAAAAAAA1mZWVfcmVjaXBpZW50AAAAAAAAEwAAAAAAAAALbWFrZXJfdG9rZW4AAAAAEwAAAAAAAAAQbWluX21ha2VyX2Ftb3VudAAAAAsAAAAAAAAAEnN0YXJ0X21ha2VyX2Ftb3VudAAAAAAACwAAAAAAAAAMdGFrZXJfYW1vdW50AAAACwAAAAAAAAALdGFrZXJfdG9rZW4AAAAAEw==",
        "AAAAAQAAAAAAAAAAAAAAC1B1c2hlZFByaWNlAAAAAAMAAAAAAAAABWVwb2NoAAAAAAAABAAAAAAAAAAFcHJpY2UAAAAAAAALAAAAAAAAAAp1cGRhdGVkX2F0AAAAAAAG",
        "AAAABAAAAAAAAAAAAAAABUVycm9yAAAAAAAAGgAAAAAAAAASQWxyZWFkeUluaXRpYWxpemVkAAAAAAABAAAAAAAAAA1Ob3RBdXRob3JpemVkAAAAAAAAAgAAAAAAAAAGUGF1c2VkAAAAAAADAAAAAAAAABBPcmRlck5vdEZpbGxhYmxlAAAABAAAAAAAAAASTm90RmlsbGFibGVCeVRha2VyAAAAAAAFAAAAAAAAABNTaWduZXJOb3RBdXRob3JpemVkAAAAAAYAAAAAAAAADUludmFsaWRBbW91bnQAAAAAAAAHAAAAAAAAAAhPdmVyZmxvdwAAAAgAAAAAAAAACk5vU2NoZWR1bGUAAAAAAAkAAAAAAAAAD0ludmFsaWRTY2hlZHVsZQAAAAAKAAAAAAAAAAdOb1ByaWNlAAAAAAsAAAAAAAAAEEJwc1BlckRheVRvb0hpZ2gAAAAMAAAAAAAAABBEaXNjb3VudFRvb0xhcmdlAAAADQAAAAAAAAAORmVlT3V0T2ZCb3VuZHMAAAAAAA4AAAAAAAAAEEJlbG93TWluUmVjZWl2ZWQAAAAPAAAAAAAAABJNYWtlckFtb3VudFRvb0hpZ2gAAAAAABAAAAAAAAAACVNhbWVUb2tlbgAAAAAAABEAAAAAAAAAEExpc3RpbmdOb3RBY3RpdmUAAAASAAAAAAAAAAtBc2tBYm92ZU1heAAAAAATAAAAAAAAAA5QcmljZURldmlhdGlvbgAAAAAAFAAAAAAAAAAVU2NoZWR1bGVTaGlmdFRvb0xhcmdlAAAAAAAAFQAAAAAAAAANSW52YWxpZENvbmZpZwAAAAAAABYAAAAAAAAAC1dyb25nU2VuZGVyAAAAABcAAAAAAAAAD1NhbHRJc0NhbmNlbGxlZAAAAAAYAAAAAAAAABFSZXF1ZXN0T3ZlcmZpbGxlZAAAAAAAABkAAAAAAAAAElByaWNlVXBkYXRlVG9vU29vbgAAAAAAGg==",
        "AAAAAgAAAAAAAAAAAAAAB0RhdGFLZXkAAAAADgAAAAAAAAAAAAAABUFkbWluAAAAAAAAAAAAAAAAAAADQ2ZnAAAAAAAAAAAAAAAAA1JlZgAAAAAAAAAAAAAAAAZQYXVzZWQAAAAAAAAAAAAAAAAABk5leHRJZAAAAAAAAQAAAAAAAAAGS2VlcGVyAAAAAAABAAAAEwAAAAEAAAAAAAAABVNjaGVkAAAAAAAAAQAAABMAAAABAAAAAAAAAAZPcmFjbGUAAAAAAAIAAAATAAAAEwAAAAEAAAAAAAAACEZhbGxiYWNrAAAAAQAAABMAAAABAAAAAAAAAAZGaWxsZWQAAAAAAAEAAAPuAAAAIAAAAAEAAAAAAAAADVJlcXVlc3RGaWxsZWQAAAAAAAABAAAD7gAAACAAAAABAAAAAAAAAA1TYWx0Q2FuY2VsbGVkAAAAAAAAAgAAABMAAAAGAAAAAQAAAAAAAAAGU2lnbmVyAAAAAAACAAAAEwAAA+4AAAAgAAAAAQAAAAAAAAAHTGlzdGluZwAAAAABAAAABg==",
        "AAAAAQAAAIhBbiBhc3NldCdzIHJlZGVtcHRpb24gc2NoZWR1bGUuIFRoZSBzZXR0bGVtZW50IGNvbnRyYWN0IGtlZXBzIG9uZSBwZXIgYXNzZXQKZm9yIG1hcmtldCBtYWtlcnM7IGEgZmFjaWxpdHkga2VlcHMgaXRzIG93biBwZXIgUldBIGl0IGJ1eXMuAAAAAAAAAAhTY2hlZHVsZQAAAAUAAAAAAAAADWN5Y2xlX3NlY29uZHMAAAAAAAAEAAAAW1JhdGUgY2VpbGluZyBmb3IgdGhpcyBhc3NldCwgaW4gaHVuZHJlZHRocyBvZiBhIGJhc2lzIHBvaW50IHBlciBkYXk6CjEwMDAgaXMgMTAuMDAgYnBzL2RheS4AAAAAD21heF9icHNfcGVyX2RheQAAAAAEAAAAAAAAAARtb2RlAAAH0AAAAAxTY2hlZHVsZU1vZGUAAAAAAAAAEm5leHRfcmVkZW1wdGlvbl9hdAAAAAAABgAAAAAAAAAPcm9sbGluZ19zZWNvbmRzAAAAAAQ=",
        "AAAAAQAAAElBbiBvcmFjbGUgcmVhZGluZzogYHByaWNlYCBpcyBxdW90ZSBwZXIgYmFzZSwgMWUxOC1zY2FsZWQgb3ZlciByYXcgdW5pdHMuAAAAAAAAAAAAAAlQcmljZURhdGEAAAAAAAACAAAAAAAAAAVwcmljZQAAAAAAAAsAAAAAAAAACXRpbWVzdGFtcAAAAAAAAAY=",
        "AAAAAQAAAAAAAAAAAAAAC1F1ZXVlQ29uZmlnAAAAAAIAAAAlV2hvIGRlcGxveWVkIGl0LCB0aHJvdWdoIHRoZSBmYWN0b3J5LgAAAAAAAAdjdXJhdG9yAAAAABMAAAAAAAAACGZhY2lsaXR5AAAAEw==",
        "AAAAAQAAADZBIHZhdWx0J3Mgc2V0dGluZ3MgYXQgY3JlYXRpb24uIGBhc3NldGAgbmV2ZXIgY2hhbmdlcy4AAAAAAAAAAAALVmF1bHRDb25maWcAAAAACQAAAAAAAAAFYXNzZXQAAAAAAAATAAAAAAAAAAdjdXJhdG9yAAAAABMAAAA/UGVyZm9ybWFuY2UgZmVlIG9uIHNoYXJlLXByaWNlIGdhaW5zIGFib3ZlIHRoZSBoaWdoLXdhdGVyIG1hcmsuAAAAAA9jdXJhdG9yX2ZlZV9icHMAAAAABAAAACtOQVYgY2VpbGluZyBmb3IgZGVwb3NpdHM7IDAgbWVhbnMgdW5jYXBwZWQuAAAAAAtkZXBvc2l0X2NhcAAAAAALAAAAKU1heSByZXZva2UgYW55IHBlbmRpbmcgdGltZWxvY2tlZCBjaGFuZ2UuAAAAAAAACGd1YXJkaWFuAAAAEwAAAAAAAAAHbWFuYWdlcgAAAAATAAAAAAAAAARuYW1lAAAAEAAAAAAAAAAGc3ltYm9sAAAAAAAQAAAAOlNlY29uZHMgYSBnb3Zlcm5lZCBjaGFuZ2Ugd2FpdHMgYmVmb3JlIGl0IGNhbiBiZSBhY2NlcHRlZC4AAAAAAAh0aW1lbG9jawAAAAY=",
        "AAAAAgAAAAAAAAAAAAAADFNjaGVkdWxlTW9kZQAAAAIAAAAAAAAANUFsd2F5cyBUK046IGByb2xsaW5nX3NlY29uZHNgIHJlZ2FyZGxlc3Mgb2YgdGhlIGRhdGUuAAAAAAAAB1JvbGxpbmcAAAAAAAAAAEVSZWRlbXB0aW9uIGxhbmRzIG9uIGEgY2FsZW5kYXIgZGF0ZSBhbmQgcm9sbHMgZm9yd2FyZCBvbmNlIGl0IHBhc3Nlcy4AAAAAAAAIQ3ljbGljYWw=",
        "AAAAAQAAAI5BIEZhY2lsaXR5IGxlZydzIGBkYXRhYDogd2hpY2ggZmFjaWxpdHksIGFuZCB3aG8gcGF5cyB0aGUgaW5wdXQuIFRoZQpmYWNpbGl0eSByZXF1aXJlcyB0aGUgcGF5ZXIncyBvd24gc2lnbmF0dXJlLCBzbyBuYW1pbmcgYW55b25lIGVsc2UgZmFpbHMuAAAAAAAAAAAADUZhY2lsaXR5Um91dGUAAAAAAAACAAAAAAAAAAhmYWNpbGl0eQAAABMAAAAAAAAABXBheWVyAAAAAAAAEw==",
        "AAAAAQAAAAAAAAAAAAAADkZhY2lsaXR5Q29uZmlnAAAAAAAJAAAAPVRoZSB2YXVsdCBob2xkaW5nIHRoZSBmYWNpbGl0eSdzIGJhc2UgYXNzZXQgKGl0cyBzdGFibGVjb2luKS4AAAAAAAAKYmFzZV92YXVsdAAAAAAAEwAAAAAAAAAHY3VyYXRvcgAAAAATAAAAQ1BlcmZvcm1hbmNlIGZlZSBvbiBvTFAgc2hhcmUtcHJpY2UgZ2FpbnMgYWJvdmUgdGhlIGhpZ2gtd2F0ZXIgbWFyay4AAAAAD2N1cmF0b3JfZmVlX2JwcwAAAAAEAAAAAAAAAAhndWFyZGlhbgAAABMAAAAAAAAAB21hbmFnZXIAAAAAEwAAAAAAAAAEbmFtZQAAABAAAAAAAAAABnN5bWJvbAAAAAAAEAAAAAAAAAAIdGltZWxvY2sAAAAGAAAAQlNlY29uZHMgb3ZlciB3aGljaCBhIHRyYWRlJ3MgZ2FpbiBhZ2FpbnN0IHRoZSBvcmFjbGUgbWFyayB1bmxvY2tzLgAAAAAADHZlc3Rfc2Vjb25kcwAAAAY=",
        "AAAABAAAAAAAAAAAAAAADVRpbWVsb2NrRXJyb3IAAAAAAAADAAAAAAAAAAlOb1BlbmRpbmcAAAAAAABuAAAAAAAAAAhOb3RSZWFkeQAAAG8AAAAAAAAAC05vdEd1YXJkaWFuAAAAAHA=",
        "AAAAAQAAAAAAAAAAAAAAB1JlcXVlc3QAAAAADAAAAAAAAAAGZXhwaXJ5AAAAAAAGAAAAAAAAAAdmZWVfYnBzAAAAAAQAAAAAAAAADWZlZV9yZWNpcGllbnQAAAAAAAATAAAAAAAAAAttYWtlcl90b2tlbgAAAAATAAAAAAAAABNtaW5fcmVjZWl2ZWRfYW1vdW50AAAAAAsAAAAAAAAACm9yZGVyX3R5cGUAAAAAB9AAAAAJT3JkZXJUeXBlAAAAAAAAAAAAAARzYWx0AAAABgAAAAAAAAAGc2VuZGVyAAAAAAPoAAAAEwAAAAAAAAAFdGFrZXIAAAAAAAPoAAAAEwAAAAAAAAAMdGFrZXJfYW1vdW50AAAACwAAAFlDZWlsaW5nIG9uIHRoZSBtYWtlcidzIHJhdGUsIGluIGh1bmRyZWR0aHMgb2YgYSBiYXNpcyBwb2ludCBwZXIgZGF5OgoyNTAgaXMgMi41MCBicHMvZGF5LgAAAAAAABV0YWtlcl9tYXhfYnBzX3Blcl9kYXkAAAAAAAAEAAAAAAAAAAt0YWtlcl90b2tlbgAAAAAT",
        "AAAAAQAAAAAAAAAAAAAACFJmcU9yZGVyAAAADgAAAAAAAAAGZXhwaXJ5AAAAAAAGAAAAAAAAAAdmZWVfYnBzAAAAAAQAAAAAAAAADWZlZV9yZWNpcGllbnQAAAAAAAATAAAAAAAAAAVtYWtlcgAAAAAAABMAAABFVGhlIGJpZCwgaW4gaHVuZHJlZHRocyBvZiBhIGJhc2lzIHBvaW50IHBlciBkYXk6IDI1MCBpcyAyLjUwIGJwcy9kYXkuAAAAAAAAEW1ha2VyX2Jwc19wZXJfZGF5AAAAAAAABAAAAAAAAAALbWFrZXJfdG9rZW4AAAAAEwAAAAAAAAAQbWF4X21ha2VyX2Ftb3VudAAAAAsAAAAAAAAAE21pbl9yZWNlaXZlZF9hbW91bnQAAAAACwAAAAAAAAAEc2FsdAAAAAYAAAAAAAAABnNlbmRlcgAAAAAD6AAAABMAAAAAAAAABXRha2VyAAAAAAAD6AAAABMAAAAAAAAADHRha2VyX2Ftb3VudAAAAAsAAABEQ2VpbGluZyBvbiB0aGUgbWFrZXIncyByYXRlLCBpbiBodW5kcmVkdGhzIG9mIGEgYmFzaXMgcG9pbnQgcGVyIGRheS4AAAAVdGFrZXJfbWF4X2Jwc19wZXJfZGF5AAAAAAAABAAAAAAAAAALdGFrZXJfdG9rZW4AAAAAEw==",
        "AAAAAgAAAAAAAAAAAAAACU9yZGVyVHlwZQAAAAAAAAIAAAAAAAAAAAAAAANSZnEAAAAAAAAAAAAAAAAFRml4ZWQAAAA=",
        "AAAAAQAAAAAAAAAAAAAACVNpZ25hdHVyZQAAAAAAAAIAAAAAAAAACXNpZ25hdHVyZQAAAAAAA+4AAABAAAAAAAAAAAZzaWduZXIAAAAAA+4AAAAg",
        "AAAAAQAAAAAAAAAAAAAACkZpbGxSZXN1bHQAAAAAAAMAAAAAAAAAA2ZlZQAAAAALAAAAAAAAAAxtYWtlcl9maWxsZWQAAAALAAAAAAAAAAx0YWtlcl9maWxsZWQAAAAL",
        "AAAAAQAAAAAAAAAAAAAACkZpeGVkT3JkZXIAAAAAAAwAAAAAAAAABmV4cGlyeQAAAAAABgAAAAAAAAAHZmVlX2JwcwAAAAAEAAAAAAAAAA1mZWVfcmVjaXBpZW50AAAAAAAAEwAAAAAAAAAFbWFrZXIAAAAAAAATAAAAAAAAAAxtYWtlcl9hbW91bnQAAAALAAAAAAAAAAttYWtlcl90b2tlbgAAAAATAAAAAAAAABNtaW5fcmVjZWl2ZWRfYW1vdW50AAAAAAsAAAAAAAAABHNhbHQAAAAGAAAAAAAAAAZzZW5kZXIAAAAAA+gAAAATAAAAAAAAAAV0YWtlcgAAAAAAA+gAAAATAAAAAAAAAAx0YWtlcl9hbW91bnQAAAALAAAAAAAAAAt0YWtlcl90b2tlbgAAAAAT" ]),
      options
    )
  }
  public readonly fromJSON = {
    upgrade: this.txFromJSON<null>,
        price_of: this.txFromJSON<i128>,
        get_admin: this.txFromJSON<string>,
        is_paused: this.txFromJSON<boolean>,
        get_config: this.txFromJSON<Config>,
        initialize: this.txFromJSON<null>,
        push_price: this.txFromJSON<null>,
        set_config: this.txFromJSON<null>,
        set_keeper: this.txFromJSON<null>,
        set_oracle: this.txFromJSON<null>,
        set_paused: this.txFromJSON<null>,
        cancel_salt: this.txFromJSON<null>,
        current_ask: this.txFromJSON<i128>,
        get_listing: this.txFromJSON<Option<Listing>>,
        get_schedule: this.txFromJSON<Option<Schedule>>,
        hash_request: this.txFromJSON<Buffer>,
        set_schedule: this.txFromJSON<null>,
        filled_amount: this.txFromJSON<i128>,
        get_reference: this.txFromJSON<Reference>,
        set_reference: this.txFromJSON<null>,
        fill_rfq_order: this.txFromJSON<FillResult>,
        hash_rfq_order: this.txFromJSON<Buffer>,
        is_order_signer: this.txFromJSON<boolean>,
        quote_rfq_order: this.txFromJSON<Quote>,
        fill_dutch_order: this.txFromJSON<FillResult>,
        fill_fixed_order: this.txFromJSON<FillResult>,
        hash_fixed_order: this.txFromJSON<Buffer>,
        is_salt_cancelled: this.txFromJSON<boolean>,
        cancel_dutch_order: this.txFromJSON<null>,
        create_dutch_order: this.txFromJSON<u64>,
        register_order_signer: this.txFromJSON<null>,
        request_filled_amount: this.txFromJSON<i128>,
        seconds_to_redemption: this.txFromJSON<u32>
  }
}