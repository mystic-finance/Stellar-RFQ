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





export type Kind = {tag: "Vault", values: void} | {tag: "Facility", values: void} | {tag: "Queue", values: void};


export interface Wasms {
  facility: Buffer;
  queue: Buffer;
  vault: Buffer;
}


export interface FacilityQuote {
  /**
 * What the trader receives, net of the protocol fee.
 */
amount_out: i128;
  facility: string;
}

export const Errors = {
  1: {message:"InvalidConfig"},
  2: {message:"NotRegistered"},
  3: {message:"TooManyActive"},
  4: {message:"AdapterNotAllowed"},
  5: {message:"FacilityNotActive"},
  6: {message:"InvalidData"},
  7: {message:"BelowMinOut"},
  8: {message:"Paused"}
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
   * Construct and simulate a fee transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  fee: (options?: MethodOptions) => Promise<AssembledTransaction<readonly [string, u32]>>

  /**
   * Construct and simulate a fill transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Router only. `data` is a `FacilityRoute`; the facility pulls the input
   * from its payer, who signed the router transaction.
   */
  fill: ({recipient, token_in, token_out, amount_in, min_amount_out, data}: {recipient: string, token_in: string, token_out: string, amount_in: i128, min_amount_out: i128, data: Buffer}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a swap transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Standalone entry: `payer` trades with `facility` and signs for it.
   */
  swap: ({payer, recipient, token_in, token_out, amount_in, min_out, facility}: {payer: string, recipient: string, token_in: string, token_out: string, amount_in: i128, min_out: i128, facility: string}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a admin transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  admin: (options?: MethodOptions) => Promise<AssembledTransaction<string>>

  /**
   * Construct and simulate a quote transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * The best active facility's net quote. Advisory: `fill` trades with the
   * facility named in the route, not necessarily this one.
   */
  quote: ({token_in, token_out, amount_in}: {token_in: string, token_out: string, amount_in: i128}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a wasms transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  wasms: (options?: MethodOptions) => Promise<AssembledTransaction<Wasms>>

  /**
   * Construct and simulate a active transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  active: (options?: MethodOptions) => Promise<AssembledTransaction<Array<string>>>

  /**
   * Construct and simulate a router transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  router: (options?: MethodOptions) => Promise<AssembledTransaction<string>>

  /**
   * Construct and simulate a kind_of transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  kind_of: ({contract}: {contract: string}, options?: MethodOptions) => Promise<AssembledTransaction<Option<Kind>>>

  /**
   * Construct and simulate a set_fee transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_fee: ({recipient, fee_bps}: {recipient: string, fee_bps: u32}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a upgrade transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  upgrade: ({wasm_hash}: {wasm_hash: Buffer}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a quote_all transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Every active facility's quote, net of the protocol fee.
   */
  quote_all: ({token_in, token_out, amount_in}: {token_in: string, token_out: string, amount_in: i128}, options?: MethodOptions) => Promise<AssembledTransaction<Array<FacilityQuote>>>

  /**
   * Construct and simulate a set_wasms transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_wasms: ({wasms}: {wasms: Wasms}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a set_active transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_active: ({facility, active}: {facility: string, active: boolean}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a set_paused transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Stops routing. Never touches a facility's or vault's own exits.
   */
  set_paused: ({paused}: {paused: boolean}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a set_router transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_router: ({router}: {router: string}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a vault_wasm transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  vault_wasm: (options?: MethodOptions) => Promise<AssembledTransaction<Buffer>>

  /**
   * Construct and simulate a create_queue transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * For one of our facilities.
   */
  create_queue: ({config}: {config: QueueConfig}, options?: MethodOptions) => Promise<AssembledTransaction<string>>

  /**
   * Construct and simulate a create_vault transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Anyone may deploy, as curator; trading is a separate, admin-gated step.
   */
  create_vault: ({config}: {config: VaultConfig}, options?: MethodOptions) => Promise<AssembledTransaction<string>>

  /**
   * Construct and simulate a allow_adapter transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * The protocol vets adapter code once; curators choose among what is allowed.
   */
  allow_adapter: ({wasm_hash, allowed}: {wasm_hash: Buffer, allowed: boolean}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a deploy_adapter transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Only one of our vaults may ask, and only for allowlisted code. The
   * adapter is owned by that vault and serves no one else.
   */
  deploy_adapter: ({vault, wasm_hash, asset, venue}: {vault: string, wasm_hash: Buffer, asset: string, venue: string}, options?: MethodOptions) => Promise<AssembledTransaction<string>>

  /**
   * Construct and simulate a create_facility transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Its base vault must be one of ours.
   */
  create_facility: ({config}: {config: FacilityConfig}, options?: MethodOptions) => Promise<AssembledTransaction<string>>

}
export class Client extends ContractClient {
  static async deploy<T = Client>(
        /** Constructor/Initialization Args for the contract's `__constructor` method */
        {admin, router, wasms, fee_recipient, fee_bps}: {admin: string, router: string, wasms: Wasms, fee_recipient: string, fee_bps: u32},
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
    return ContractClient.deploy({admin, router, wasms, fee_recipient, fee_bps}, options)
  }
  constructor(public readonly options: ContractClientOptions) {
    super(
      new ContractSpec([ "AAAAAAAAAAAAAAADZmVlAAAAAAAAAAABAAAD7QAAAAIAAAATAAAABA==",
        "AAAAAAAAAHlSb3V0ZXIgb25seS4gYGRhdGFgIGlzIGEgYEZhY2lsaXR5Um91dGVgOyB0aGUgZmFjaWxpdHkgcHVsbHMgdGhlIGlucHV0CmZyb20gaXRzIHBheWVyLCB3aG8gc2lnbmVkIHRoZSByb3V0ZXIgdHJhbnNhY3Rpb24uAAAAAAAABGZpbGwAAAAGAAAAAAAAAAlyZWNpcGllbnQAAAAAAAATAAAAAAAAAAh0b2tlbl9pbgAAABMAAAAAAAAACXRva2VuX291dAAAAAAAABMAAAAAAAAACWFtb3VudF9pbgAAAAAAAAsAAAAAAAAADm1pbl9hbW91bnRfb3V0AAAAAAALAAAAAAAAAARkYXRhAAAADgAAAAEAAAAL",
        "AAAAAAAAAEJTdGFuZGFsb25lIGVudHJ5OiBgcGF5ZXJgIHRyYWRlcyB3aXRoIGBmYWNpbGl0eWAgYW5kIHNpZ25zIGZvciBpdC4AAAAAAARzd2FwAAAABwAAAAAAAAAFcGF5ZXIAAAAAAAATAAAAAAAAAAlyZWNpcGllbnQAAAAAAAATAAAAAAAAAAh0b2tlbl9pbgAAABMAAAAAAAAACXRva2VuX291dAAAAAAAABMAAAAAAAAACWFtb3VudF9pbgAAAAAAAAsAAAAAAAAAB21pbl9vdXQAAAAACwAAAAAAAAAIZmFjaWxpdHkAAAATAAAAAQAAAAs=",
        "AAAAAAAAAAAAAAAFYWRtaW4AAAAAAAAAAAAAAQAAABM=",
        "AAAAAAAAAH1UaGUgYmVzdCBhY3RpdmUgZmFjaWxpdHkncyBuZXQgcXVvdGUuIEFkdmlzb3J5OiBgZmlsbGAgdHJhZGVzIHdpdGggdGhlCmZhY2lsaXR5IG5hbWVkIGluIHRoZSByb3V0ZSwgbm90IG5lY2Vzc2FyaWx5IHRoaXMgb25lLgAAAAAAAAVxdW90ZQAAAAAAAAMAAAAAAAAACHRva2VuX2luAAAAEwAAAAAAAAAJdG9rZW5fb3V0AAAAAAAAEwAAAAAAAAAJYW1vdW50X2luAAAAAAAACwAAAAEAAAAL",
        "AAAAAAAAAAAAAAAFd2FzbXMAAAAAAAAAAAAAAQAAB9AAAAAFV2FzbXMAAAA=",
        "AAAAAAAAAAAAAAAGYWN0aXZlAAAAAAAAAAAAAQAAA+oAAAAT",
        "AAAAAAAAAAAAAAAGcm91dGVyAAAAAAAAAAAAAQAAABM=",
        "AAAAAgAAAAAAAAAAAAAABEtpbmQAAAADAAAAAAAAAAAAAAAFVmF1bHQAAAAAAAAAAAAAAAAAAAhGYWNpbGl0eQAAAAAAAAAAAAAABVF1ZXVlAAAA",
        "AAAAAAAAAAAAAAAHa2luZF9vZgAAAAABAAAAAAAAAAhjb250cmFjdAAAABMAAAABAAAD6AAAB9AAAAAES2luZA==",
        "AAAAAAAAAAAAAAAHc2V0X2ZlZQAAAAACAAAAAAAAAAlyZWNpcGllbnQAAAAAAAATAAAAAAAAAAdmZWVfYnBzAAAAAAQAAAAA",
        "AAAAAAAAAAAAAAAHdXBncmFkZQAAAAABAAAAAAAAAAl3YXNtX2hhc2gAAAAAAAPuAAAAIAAAAAA=",
        "AAAAAQAAAAAAAAAAAAAABVdhc21zAAAAAAAAAwAAAAAAAAAIZmFjaWxpdHkAAAPuAAAAIAAAAAAAAAAFcXVldWUAAAAAAAPuAAAAIAAAAAAAAAAFdmF1bHQAAAAAAAPuAAAAIA==",
        "AAAAAAAAADdFdmVyeSBhY3RpdmUgZmFjaWxpdHkncyBxdW90ZSwgbmV0IG9mIHRoZSBwcm90b2NvbCBmZWUuAAAAAAlxdW90ZV9hbGwAAAAAAAADAAAAAAAAAAh0b2tlbl9pbgAAABMAAAAAAAAACXRva2VuX291dAAAAAAAABMAAAAAAAAACWFtb3VudF9pbgAAAAAAAAsAAAABAAAD6gAAB9AAAAANRmFjaWxpdHlRdW90ZQAAAA==",
        "AAAAAAAAAAAAAAAJc2V0X3dhc21zAAAAAAAAAQAAAAAAAAAFd2FzbXMAAAAAAAfQAAAABVdhc21zAAAAAAAAAA==",
        "AAAAAAAAAAAAAAAKc2V0X2FjdGl2ZQAAAAAAAgAAAAAAAAAIZmFjaWxpdHkAAAATAAAAAAAAAAZhY3RpdmUAAAAAAAEAAAAA",
        "AAAAAAAAAD9TdG9wcyByb3V0aW5nLiBOZXZlciB0b3VjaGVzIGEgZmFjaWxpdHkncyBvciB2YXVsdCdzIG93biBleGl0cy4AAAAACnNldF9wYXVzZWQAAAAAAAEAAAAAAAAABnBhdXNlZAAAAAAAAQAAAAA=",
        "AAAAAAAAAAAAAAAKc2V0X3JvdXRlcgAAAAAAAQAAAAAAAAAGcm91dGVyAAAAAAATAAAAAA==",
        "AAAAAAAAAAAAAAAKdmF1bHRfd2FzbQAAAAAAAAAAAAEAAAPuAAAAIA==",
        "AAAAAAAAABpGb3Igb25lIG9mIG91ciBmYWNpbGl0aWVzLgAAAAAADGNyZWF0ZV9xdWV1ZQAAAAEAAAAAAAAABmNvbmZpZwAAAAAH0AAAAAtRdWV1ZUNvbmZpZwAAAAABAAAAEw==",
        "AAAAAAAAAEdBbnlvbmUgbWF5IGRlcGxveSwgYXMgY3VyYXRvcjsgdHJhZGluZyBpcyBhIHNlcGFyYXRlLCBhZG1pbi1nYXRlZCBzdGVwLgAAAAAMY3JlYXRlX3ZhdWx0AAAAAQAAAAAAAAAGY29uZmlnAAAAAAfQAAAAC1ZhdWx0Q29uZmlnAAAAAAEAAAAT",
        "AAAAAAAAAEtUaGUgcHJvdG9jb2wgdmV0cyBhZGFwdGVyIGNvZGUgb25jZTsgY3VyYXRvcnMgY2hvb3NlIGFtb25nIHdoYXQgaXMgYWxsb3dlZC4AAAAADWFsbG93X2FkYXB0ZXIAAAAAAAACAAAAAAAAAAl3YXNtX2hhc2gAAAAAAAPuAAAAIAAAAAAAAAAHYWxsb3dlZAAAAAABAAAAAA==",
        "AAAAAAAAAAAAAAANX19jb25zdHJ1Y3RvcgAAAAAAAAUAAAAAAAAABWFkbWluAAAAAAAAEwAAAAAAAAAGcm91dGVyAAAAAAATAAAAAAAAAAV3YXNtcwAAAAAAB9AAAAAFV2FzbXMAAAAAAAAAAAAADWZlZV9yZWNpcGllbnQAAAAAAAATAAAAAAAAAAdmZWVfYnBzAAAAAAQAAAAA",
        "AAAAAAAAAHlPbmx5IG9uZSBvZiBvdXIgdmF1bHRzIG1heSBhc2ssIGFuZCBvbmx5IGZvciBhbGxvd2xpc3RlZCBjb2RlLiBUaGUKYWRhcHRlciBpcyBvd25lZCBieSB0aGF0IHZhdWx0IGFuZCBzZXJ2ZXMgbm8gb25lIGVsc2UuAAAAAAAADmRlcGxveV9hZGFwdGVyAAAAAAAEAAAAAAAAAAV2YXVsdAAAAAAAABMAAAAAAAAACXdhc21faGFzaAAAAAAAA+4AAAAgAAAAAAAAAAVhc3NldAAAAAAAABMAAAAAAAAABXZlbnVlAAAAAAAAEwAAAAEAAAAT",
        "AAAAAAAAACNJdHMgYmFzZSB2YXVsdCBtdXN0IGJlIG9uZSBvZiBvdXJzLgAAAAAPY3JlYXRlX2ZhY2lsaXR5AAAAAAEAAAAAAAAABmNvbmZpZwAAAAAH0AAAAA5GYWNpbGl0eUNvbmZpZwAAAAAAAQAAABM=",
        "AAAAAQAAAAAAAAAAAAAADUZhY2lsaXR5UXVvdGUAAAAAAAACAAAAMldoYXQgdGhlIHRyYWRlciByZWNlaXZlcywgbmV0IG9mIHRoZSBwcm90b2NvbCBmZWUuAAAAAAAKYW1vdW50X291dAAAAAAACwAAAAAAAAAIZmFjaWxpdHkAAAAT",
        "AAAABAAAAAAAAAAAAAAABUVycm9yAAAAAAAACAAAAAAAAAANSW52YWxpZENvbmZpZwAAAAAAAAEAAAAAAAAADU5vdFJlZ2lzdGVyZWQAAAAAAAACAAAAAAAAAA1Ub29NYW55QWN0aXZlAAAAAAAAAwAAAAAAAAARQWRhcHRlck5vdEFsbG93ZWQAAAAAAAAEAAAAAAAAABFGYWNpbGl0eU5vdEFjdGl2ZQAAAAAAAAUAAAAAAAAAC0ludmFsaWREYXRhAAAAAAYAAAAAAAAAC0JlbG93TWluT3V0AAAAAAcAAAAAAAAABlBhdXNlZAAAAAAACA==",
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
    fee: this.txFromJSON<readonly [string, u32]>,
        fill: this.txFromJSON<i128>,
        swap: this.txFromJSON<i128>,
        admin: this.txFromJSON<string>,
        quote: this.txFromJSON<i128>,
        wasms: this.txFromJSON<Wasms>,
        active: this.txFromJSON<Array<string>>,
        router: this.txFromJSON<string>,
        kind_of: this.txFromJSON<Option<Kind>>,
        set_fee: this.txFromJSON<null>,
        upgrade: this.txFromJSON<null>,
        quote_all: this.txFromJSON<Array<FacilityQuote>>,
        set_wasms: this.txFromJSON<null>,
        set_active: this.txFromJSON<null>,
        set_paused: this.txFromJSON<null>,
        set_router: this.txFromJSON<null>,
        vault_wasm: this.txFromJSON<Buffer>,
        create_queue: this.txFromJSON<string>,
        create_vault: this.txFromJSON<string>,
        allow_adapter: this.txFromJSON<null>,
        deploy_adapter: this.txFromJSON<string>,
        create_facility: this.txFromJSON<string>
  }
}