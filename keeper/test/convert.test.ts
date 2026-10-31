import assert from "node:assert/strict";
import { test } from "node:test";
import { changeBps, decide, toOraclePrice } from "../src/convert.js";

test("JAAA NAV converts to Stellar raw units", () => {
  // 1.052397 USDC (6 dp) per JAAA → USDC at 7 dp, JAAA at 18 dp on Stellar.
  assert.equal(toOraclePrice({ value: 1_052_397n, decimals: 6, timestamp: 0 }, 18, 7), 10_523_970n);
  // Same decimals both sides: the 1e18 scale is the NAV itself.
  assert.equal(toOraclePrice({ value: 1_500_000n, decimals: 6, timestamp: 0 }, 7, 7), 1_500_000_000_000_000_000n);
});

test("change is measured in bps, rounded up", () => {
  assert.equal(changeBps(10_000n, 10_001n), 1n);
  assert.equal(changeBps(10_000n, 9_800n), 200n);
});

test("pushes on a move or a heartbeat, never past the oracle bound", () => {
  const opts = { minChangeBps: 5, heartbeatSeconds: 3600, maxDeviationBps: 200 };
  assert.equal(decide(null, 1n, 0, opts).push, true);
  const last = { price: 1_000_000n, timestamp: 1_000 };
  assert.equal(decide(last, 1_000_100n, 1_100, opts).push, false, "1 bp: wait");
  assert.equal(decide(last, 1_000_600n, 1_100, opts).push, true, "6 bps: push");
  assert.equal(decide(last, 1_000_100n, 4_600, opts).push, true, "heartbeat");
  assert.equal(decide(last, 1_030_000n, 1_100, opts).push, false, "300 bps: admin only");
});
