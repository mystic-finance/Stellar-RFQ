import { test } from "node:test";
import assert from "node:assert/strict";
import { Keypair, hash, xdr, scValToNative } from "@stellar/stellar-sdk";
import { Buffer } from "buffer";
import {
  ONE, sellQuote, buyQuote, formatUnits, parseUnits, formatRate, rateApr,
  signOrderHash, facilityLegData, dexLegData, aquariusHopData, Octarine, networks,
} from "../src/index.js";

const DAY = 86_400;
const C = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";
const G = Keypair.random().publicKey();

test("previews match the facility's own test vectors", () => {
  // contracts/facility/src/test.rs: 10 bps/day over 30 days is 3%.
  assert.equal(sellQuote(100_000n, ONE, 1000, 30 * DAY), 97_000n);
  // 5 bps/day over 30 days: 98,500 buys 100,000 units.
  assert.equal(buyQuote(98_500n, ONE, 500, 30 * DAY), 100_000n);
  assert.equal(sellQuote(1n, ONE, 1_000_000, 10_000 * DAY), 0n, "a discount past 100% pays nothing");
});

test("units and rates format both ways", () => {
  assert.equal(formatUnits(12_345_678n, 7), "1.2345678");
  assert.equal(formatUnits(10_000_000n, 7), "1");
  assert.equal(parseUnits("0.5", 7), 5_000_000n);
  assert.throws(() => parseUnits("0.12345678", 7));
  assert.equal(formatRate(1000), "10.00 bps/day");
  assert.ok(Math.abs(rateApr(1000) - 36.5) < 1e-9);
});

test("order signatures verify against the SEP-53 digest", () => {
  const kp = Keypair.random();
  const h = Buffer.alloc(32, 7);
  const { signer, signature } = signOrderHash(kp, h);
  assert.deepEqual(signer, Buffer.from(kp.rawPublicKey()));
  const digest = hash(Buffer.concat([Buffer.from("Stellar Signed Message:\n"), h]));
  assert.ok(kp.verify(digest, signature));
  assert.throws(() => signOrderHash(kp, Buffer.alloc(31)));
});

test("leg data decodes to the contract types", () => {
  const f = scValToNative(xdr.ScVal.fromXDR(facilityLegData({ facility: C, payer: G })));
  assert.deepEqual(f, { facility: C, payer: G });
  const d = scValToNative(xdr.ScVal.fromXDR(dexLegData({ tag: "Splits", values: [[{ amount_in: 5n, hops: [{ venue: C, token_out: C, data: aquariusHopData(C) }] }]] })));
  assert.equal(d[0], "Splits");
  assert.equal(scValToNative(xdr.ScVal.fromXDR(aquariusHopData(C))), C);
});

test("the client resolves published addresses and refuses missing ones", () => {
  const sdk = new Octarine({ network: "testnet" });
  assert.equal(sdk.networkPassphrase, networks.testnet.networkPassphrase);
  assert.equal(sdk.facility(C).options.contractId, C);
  const bare = new Octarine({ network: { rpcUrl: "https://example.org", networkPassphrase: "x" } });
  assert.throws(() => bare.router(), /no router address/);
});
