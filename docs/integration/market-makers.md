# Market makers: bidding on Octarine RFQs

A seller posts a request to sell `amount` of an RWA (or any listed token) for a
stable. Market makers bid a **rate**: a discount in bps per day over the RWA's
time to redemption. The best bid wins, and the seller fills it on-chain through
the router in one transaction. Your tokens move only at that fill, straight
between you and the seller, and only up to what you signed.

This guide uses the SDK (see [sdk.md](sdk.md)). `sdk/examples/market-maker.ts`
is a complete bot.

## 1. Get set up

1. **API key.** Ask the Octarine team to register your Stellar address as a
   market maker. Send the key as `x-api-key` on every market-maker call and in
   the WebSocket handshake. Reads are public.
2. **Inventory.** Hold the token you pay with (the request's `redemptionAsset`,
   usually USDC). Open a trustline for each token you will buy, because sellers
   pay you the request's `redeemAsset`.
3. **Allowance.** Approve the **settlement contract** (`rfq`), not the router,
   to spend your payout token. One standing approval covers every fill until
   it expires.
4. **Delegated key (optional).** To sign bids with a hot key instead of your
   treasury key, register it on-chain. A contract account (a multisig) must do
   this, because it has no key of its own.

```ts
import { prepareMaker } from "@octarine/stellar-sdk";
const maker = sdk.as(keypairSigner(TREASURY_SECRET, networks.testnet.networkPassphrase));
await prepareMaker(maker, { makerToken: USDC, amount: 100_000_0000000n, delegate: hotKeypair });
```

## 2. Find requests

```ts
import { AuctionClient, STELLAR_CHAIN_IDS } from "@octarine/stellar-sdk";
const auction = new AuctionClient({ baseUrl: API_URL, apiKey: API_KEY, chainId: STELLAR_CHAIN_IDS.testnet });

// Poll…
const { data } = await auction.requests({ marketMaker: me, excludeBidOn: true });
// …or stream (socket.io, namespace /octarine). Events arrive ~5 s after the change.
const stop = await auction.subscribe({
  onRequest: (r) => consider(r),
  onBidStatus: (e) => console.log(e.bidId, e.newStatus),   // pending → accepted | cancelled | expired
});
```

The fields of a request you price on:

| Field | |
|---|---|
| `redeemAsset` / `redemptionAsset` | token you receive / token you pay |
| `redeemAmount` | how much the seller sells (raw units) |
| `metadata.horizonSeconds` | time to the asset's next redemption, from its on-chain schedule |
| `metadata.maxBpsPerDay` | the most a bid may charge (the seller's slippage cap, bounded by the asset's schedule) |
| `metadata.indicativeMakerAmount` | what you would pay at the cap; size your inventory check on it |
| `biddingCloseTime` | bids close here |

## 3. Bid

```ts
import { placeBid } from "@octarine/stellar-sdk";
const res = await placeBid(auction, sdk, signerKeypair, {
  requestId: r.requestId,
  maker: me,               // the account whose funds pay (your treasury)
  makerBpsPerDay: 2.25,    // human bps/day; ≤ metadata.maxBpsPerDay
});
// res.data = { bidId, makerAmount, price, status: "pending", … }
```

`placeBid` runs these steps:

1. **Quote.** `POST /octarine/bid/quote` turns the rate into the exact
   `RfqOrder`.
2. **Check the order.** It confirms the order names you, carries your rate, and
   has a ceiling no higher than yours.
3. **Check the hash.** It recomputes the order hash on the settlement contract
   (`hash_rfq_order`) and compares it with the API's.
4. **Sign** the hash (SEP-53).
5. **Submit** with `POST /octarine/bid`.

You never sign a hash you have not checked. A new bid on the same request
replaces your previous one. Withdraw a bid with `auction.cancelBid(bidId, me)`.

**What you sign.** An RFQ order fixes:

- the tokens and the seller's amount;
- your rate (`maker_bps_per_day`);
- a ceiling on what you pay (`max_maker_amount`);
- the router as the only allowed submitter (`sender`);
- an expiry.

The amount you pay is computed **at fill time** from the live horizon and the
asset's price, and it never exceeds the ceiling. A partial fill scales
pro-rata.

**Off-chain vs on-chain.** Cancelling a bid off-chain stops the API from showing
it, but the signed order stays fillable until its expiry. To kill it on-chain,
call `rfq.cancel_salt(caller, signer, salt)` with the request's salt.

## 4. Settlement

When the seller picks your bid, their wallet signs two transactions the API
built:

1. an approval to settlement;
2. `router.fill` with your order as an `Rfq` leg.

Settlement verifies both signatures, pulls the seller's tokens to you, and pays
your payout (less the 10 bps protocol fee) to the seller, all in one step. You
do nothing at fill time. Keep the inventory and the allowance in place until
your bids expire.

You hear about the outcome on the WebSocket (`bid_status_update`) or by polling
`auction.makerBids(me)`.

## Errors from `/bid` and `/bid/quote`

| Error | Meaning |
|---|---|
| `BpsPerDayTooHigh` (contract #12) | your rate is above the request's `maxBpsPerDay`; the response includes the cap |
| contract #13 | the rate discounts the whole order away |
| contract #11 / #9 | no price, or no redemption schedule, for the asset |
| 400 with `expectedOrderHash` | the signed hash does not match the order the API rebuilt; requote |
| 401 | missing or inactive API key |

## Pricing, briefly

Your payout is roughly `amount × price × (1 − rate × horizon)`, where `price` is
the asset's oracle price in the payout token. A rate of 2.5 bps/day on a 30-day
horizon is a 0.75% discount, and you earn it when the RWA redeems at par. The
example bot reads a per-token rate table. A production bot prices from its cost
of capital, its redemption risk, and the inventory it already holds. That is the
loop `octarine-bidder` runs on EVM.
