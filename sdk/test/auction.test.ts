import { test } from "node:test";
import assert from "node:assert/strict";
import { Keypair } from "@stellar/stellar-sdk";
import { AuctionClient, AuctionError, packSignature, toRfqOrder, signOrderHash } from "../src/index.js";

const C = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";

function fakeFetch(responses: Record<string, unknown>, seen: { url: string; init: RequestInit }[]) {
  return (async (url: string, init: RequestInit) => {
    seen.push({ url, init });
    const key = Object.keys(responses).find((k) => url.includes(k))!;
    const body = responses[key];
    const status = (body as { __status?: number })?.__status ?? 200;
    return new Response(JSON.stringify(body), { status });
  }) as unknown as typeof fetch;
}

test("bids go out with the API key and the packed signature", async () => {
  const seen: { url: string; init: RequestInit }[] = [];
  const a = new AuctionClient({ baseUrl: "https://api.test/", apiKey: "k1", fetch: fakeFetch({ "/octarine/bid": { success: true, data: { bidId: "b1" } } }, seen) });
  const kp = Keypair.random();
  const sig = signOrderHash(kp, Buffer.alloc(32, 1));
  const r = await a.submitBid({ requestId: "r1", maker: kp.publicKey(), makerBpsPerDay: 2.5, maxMakerAmount: 99n, signature: sig });
  assert.equal(r.data.bidId, "b1");
  assert.equal(seen[0].url, "https://api.test/octarine/bid");
  assert.equal((seen[0].init.headers as Record<string, string>)["x-api-key"], "k1");
  const body = JSON.parse(String(seen[0].init.body));
  assert.deepEqual(body.signature, packSignature(sig));
  assert.equal(body.signature.signatureType, 7);
  assert.equal(body.signature.r, Buffer.from(kp.rawPublicKey()).toString("hex"));
  assert.equal(body.maxMakerAmount, "99");
});

test("requests are scoped to the chain and errors carry the API message", async () => {
  const seen: { url: string; init: RequestInit }[] = [];
  const a = new AuctionClient({ baseUrl: "https://api.test", fetch: fakeFetch({ "/octarine/requests": { data: [], page: 1, limit: 20, totalPages: 0, totalItems: 0 }, "/octarine/bid/quote": { __status: 400, message: "BpsPerDayTooHigh" } }, seen) });
  await a.requests({ status: "bidding" });
  assert.match(seen[0].url, /chainId=100101&status=bidding/);
  await assert.rejects(a.quoteBid({ requestId: "r", makerBpsPerDay: 99 }), (e: unknown) => e instanceof AuctionError && e.status === 400 && /BpsPerDayTooHigh/.test(e.message));
});

test("the API's order JSON becomes the contract type", () => {
  const o = toRfqOrder({ expiry: "10", fee_bps: 10, fee_recipient: C, maker: C, maker_bps_per_day: 250, maker_token: C, max_maker_amount: "5", min_received_amount: "4", salt: "7", sender: C, taker: "", taker_amount: "6", taker_max_bps_per_day: 900, taker_token: C });
  assert.equal(o.expiry, 10n);
  assert.equal(o.taker, undefined);
  assert.equal(o.sender, C);
  assert.equal(o.max_maker_amount, 5n);
});
