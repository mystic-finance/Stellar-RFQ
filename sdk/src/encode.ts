import { Address, hash, xdr } from "@stellar/stellar-sdk";
import { Buffer } from "buffer";
import * as dexAggregator from "./contracts/dex_aggregator.js";
import * as facilityAggregator from "./contracts/facility_aggregator.js";

const udt = (name: string) => xdr.ScSpecTypeDef.scSpecTypeUdt(new xdr.ScSpecTypeUdt({ name }));
// A client is only needed for its spec; nothing here touches the network.
const offline = { contractId: "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM", rpcUrl: "https://offline.invalid", networkPassphrase: "" };
const dex = new dexAggregator.Client(offline).spec;
const fac = new facilityAggregator.Client(offline).spec;

/** `data` for a router Facility leg: which facility, and who pays (that account signs the router call). */
export function facilityLegData(route: { facility: string; payer: string }): Buffer {
  return Buffer.from(fac.nativeToScVal(route, udt("FacilityRoute")).toXDR());
}

/** `data` for a router Dex leg: adapter splits, or SimpleSwap-style calldata. */
export function dexLegData(route: dexAggregator.Route): Buffer {
  return Buffer.from(dex.nativeToScVal(route, udt("Route")).toXDR());
}

/** A hop's `data` for the Aquarius adapter: the pool to swap through. */
export function aquariusHopData(pool: string): Buffer {
  return Buffer.from(new Address(pool).toScVal().toXDR());
}

/** A hop's `data` for the Soroswap adapter: none, the router finds the pair. */
export const soroswapHopData = (): Buffer => Buffer.alloc(0);

/** Timelock kind of a facility's `Change::Rwa` for `rwa`, to look it up with `pending(kind)`. */
export function rwaChangeKind(rwa: string): number {
  const h = Buffer.from(hash(Buffer.from(new Address(rwa).toScVal().toXDR())));
  return (0x8000_0000 | h.readUInt32BE(0)) >>> 0;
}
