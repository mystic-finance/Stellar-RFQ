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





export const Errors = {
  1: {message:"InvalidAmount"},
  2: {message:"NotFound"}
}


export interface Position {
  owner: string;
  receiver: string;
  /**
 * Shares still waiting; they keep their exposure until served.
 */
shares: i128;
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
   * Construct and simulate a bounds transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Next position to serve, and the next id to hand out. Equal when empty.
   */
  bounds: (options?: MethodOptions) => Promise<AssembledTransaction<readonly [u64, u64]>>

  /**
   * Construct and simulate a cancel transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns a position's unserved shares to its owner.
   */
  cancel: ({id}: {id: u64}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a config transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  config: (options?: MethodOptions) => Promise<AssembledTransaction<QueueConfig>>

  /**
   * Construct and simulate a process transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Serves up to `max` positions from the head with what the facility can
   * pay now. Anyone may call it.
   */
  process: ({max}: {max: u32}, options?: MethodOptions) => Promise<AssembledTransaction<u32>>

  /**
   * Construct and simulate a request transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Moves `owner`'s oLP into the queue; it redeems to `receiver` when served.
   */
  request: ({owner, receiver, shares}: {owner: string, receiver: string, shares: i128}, options?: MethodOptions) => Promise<AssembledTransaction<u64>>

  /**
   * Construct and simulate a position transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  position: ({id}: {id: u64}, options?: MethodOptions) => Promise<AssembledTransaction<Option<Position>>>

}
export class Client extends ContractClient {
  static async deploy<T = Client>(
        /** Constructor/Initialization Args for the contract's `__constructor` method */
        {config}: {config: QueueConfig},
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
    return ContractClient.deploy({config}, options)
  }
  constructor(public readonly options: ContractClientOptions) {
    super(
      new ContractSpec([ "AAAAAAAAAEZOZXh0IHBvc2l0aW9uIHRvIHNlcnZlLCBhbmQgdGhlIG5leHQgaWQgdG8gaGFuZCBvdXQuIEVxdWFsIHdoZW4gZW1wdHkuAAAAAAAGYm91bmRzAAAAAAAAAAAAAQAAA+0AAAACAAAABgAAAAY=",
        "AAAAAAAAADJSZXR1cm5zIGEgcG9zaXRpb24ncyB1bnNlcnZlZCBzaGFyZXMgdG8gaXRzIG93bmVyLgAAAAAABmNhbmNlbAAAAAAAAQAAAAAAAAACaWQAAAAAAAYAAAAA",
        "AAAAAAAAAAAAAAAGY29uZmlnAAAAAAAAAAAAAQAAB9AAAAALUXVldWVDb25maWcA",
        "AAAAAAAAAGJTZXJ2ZXMgdXAgdG8gYG1heGAgcG9zaXRpb25zIGZyb20gdGhlIGhlYWQgd2l0aCB3aGF0IHRoZSBmYWNpbGl0eSBjYW4KcGF5IG5vdy4gQW55b25lIG1heSBjYWxsIGl0LgAAAAAAB3Byb2Nlc3MAAAAAAQAAAAAAAAADbWF4AAAAAAQAAAABAAAABA==",
        "AAAAAAAAAElNb3ZlcyBgb3duZXJgJ3Mgb0xQIGludG8gdGhlIHF1ZXVlOyBpdCByZWRlZW1zIHRvIGByZWNlaXZlcmAgd2hlbiBzZXJ2ZWQuAAAAAAAAB3JlcXVlc3QAAAAAAwAAAAAAAAAFb3duZXIAAAAAAAATAAAAAAAAAAhyZWNlaXZlcgAAABMAAAAAAAAABnNoYXJlcwAAAAAACwAAAAEAAAAG",
        "AAAABAAAAAAAAAAAAAAABUVycm9yAAAAAAAAAgAAAAAAAAANSW52YWxpZEFtb3VudAAAAAAAAAEAAAAAAAAACE5vdEZvdW5kAAAAAg==",
        "AAAAAAAAAAAAAAAIcG9zaXRpb24AAAABAAAAAAAAAAJpZAAAAAAABgAAAAEAAAPoAAAH0AAAAAhQb3NpdGlvbg==",
        "AAAAAQAAAAAAAAAAAAAACFBvc2l0aW9uAAAAAwAAAAAAAAAFb3duZXIAAAAAAAATAAAAAAAAAAhyZWNlaXZlcgAAABMAAAA8U2hhcmVzIHN0aWxsIHdhaXRpbmc7IHRoZXkga2VlcCB0aGVpciBleHBvc3VyZSB1bnRpbCBzZXJ2ZWQuAAAABnNoYXJlcwAAAAAACw==",
        "AAAAAAAAAAAAAAANX19jb25zdHJ1Y3RvcgAAAAAAAAEAAAAAAAAABmNvbmZpZwAAAAAH0AAAAAtRdWV1ZUNvbmZpZwAAAAAA",
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
    bounds: this.txFromJSON<readonly [u64, u64]>,
        cancel: this.txFromJSON<null>,
        config: this.txFromJSON<QueueConfig>,
        process: this.txFromJSON<u32>,
        request: this.txFromJSON<u64>,
        position: this.txFromJSON<Option<Position>>
  }
}