# Selling and swapping: router, DEX aggregator, facility legs

This guide is for **takers**, and for the frontends and backends that act for
them: wallets, solvers, the Octarine app. A taker sells `taker_token` and
receives `maker_token`. A trade can be one source or several, and every route
settles in one transaction through the **router**:

| Leg | Liquidity | `data` |
|---|---|---|
| `Rfq` | a market maker's signed order (see [market-makers.md](market-makers.md)) | the order + signatures |
| `Dex` | Soroswap, Aquarius, … through the DEX aggregator | `dexLegData(route)` |
| `Facility` | a curated facility (see [facilities.md](facilities.md)) | `facilityLegData({ facility, payer })` |

The taker sets one `min_out` over the whole route. If the route delivers less,
the transaction reverts and nothing moves. The route is chosen off-chain and
executed exactly as given; the router never substitutes another.

## Quoting

```ts
const r = sdk.router();
const all  = (await r.quote({ taker_token: RWA, maker_token: USDC, taker_amount })).result;      // every registered source
const best = (await r.best_quote({ taker_token: RWA, maker_token: USDC, taker_amount })).result;

// Per source, with detail:
const facilities = (await sdk.facilityAggregator().quote_all({ token_in: RWA, token_out: USDC, amount_in })).result; // [{ facility, amount_out }]
const viaRoute   = (await sdk.dexAggregator().quote_route({ token_in, token_out, amount_in, route: splits, partner: undefined })).result;
```

Every quote is net of the protocol fees. RFQ bids come from the auction API, not
the chain.

## Approvals

| Route contains | The taker must |
|---|---|
| an RFQ bid quoted to the taker (`order.taker` = taker) | approve **settlement** (`rfq`) |
| an open RFQ bid, or any Dex leg | approve the **router** |
| a Facility leg | nothing: the facility pulls under the taker's signature on the router call |

```ts
const ledger = (await new rpc.Server(sdk.rpcUrl).getLatestLedger()).sequence;
await (await taker.token(XLM).approve({ from: me, spender: sdk.contracts.router!, amount, expiration_ledger: ledger + 1000 })).signAndSend();
```

A Stellar classic asset (USDC, for example) needs a **trustline** before an
account can receive it: a `changeTrust` operation, signed once.

## Router fill

```ts
import { dexLegData, facilityLegData, soroswapHopData, aquariusHopData } from "@octarine/stellar-sdk";

const legs = [
  { tag: "Facility", values: [{
      aggregator: sdk.contracts.facilityAggregator!,
      taker_amount: 600_0000000n,
      min_maker_amount: facilityQuote,
      data: facilityLegData({ facility: FACILITY, payer: me }),   // payer = the taker, who signs this tx
  }] },
  { tag: "Dex", values: [{
      aggregator: sdk.contracts.dexAggregator!,
      taker_amount: 400_0000000n,
      min_maker_amount: dexQuote,
      data: dexLegData({ tag: "Splits", values: [[
        { amount_in: 400_0000000n, hops: [{ venue: sdk.contracts.soroswapAdapter!, token_out: USDC, data: soroswapHopData() }] },
      ]] }),
  }] },
];
const { result } = await (await taker.router().fill({
  taker: me, taker_token: RWA, maker_token: USDC, route: legs, min_out: total * 995n / 1000n,
})).signAndSend();
// result = { taker_spent, amount_out }
```

The router checks the **taker's own balance change** against `min_out`, so legs
that pay the taker directly and legs that pay through the router count the same.
The router holds nothing between transactions.

## DEX aggregator on its own

The DEX aggregator has two direct entry points.

**Adapter routes.** These are splits and hops through allowlisted adapters.
Each split takes `amount_in` and walks its hops in order:

```ts
const res = await (await taker.dexAggregator().swap({
  user: me,
  desc: { token_in: XLM, token_out: USDC, amount_in, min_out, to: me },
  route: [
    { amount_in: half, hops: [{ venue: sdk.contracts.soroswapAdapter!, token_out: USDC, data: soroswapHopData() }] },
    { amount_in: half, hops: [{ venue: sdk.contracts.aquariusAdapter!, token_out: USDC, data: aquariusHopData(AQUARIUS_POOL) }] },
  ],
  partner: { address: INTEGRATOR, fee_bps: 25 },   // optional integrator fee, ≤ max_partner_fee_bps
})).signAndSend();
// res.result = { amount_out, gross_out, protocol_fee, partner_fee }
```

**Calldata routes** (`swap_calls`, ParaSwap SimpleSwap style). Each entry is a
call an off-chain router built: `{target, function, args, auth}`.

- `target` must be on the aggregator's allowlist (`allow_target`), and can never
  be `token_in` or `token_out`.
- `auth` may only authorise `token_in.transfer(from = aggregator, …)`, and the
  total may not exceed `amount_in`.

The aggregator measures what it actually received, takes its fees, pays `to`,
and refunds any unspent input.

**Fees.** Both modes take the protocol fee (`fees().protocol_fee_bps`) plus the
partner fee from the output. Fees accrue inside the aggregator. Each recipient
(the protocol, and every partner) reads its balance with `owed(recipient, token)`
and withdraws with `claim(recipient, tokens)`. A partner's cut of the protocol
fee is `partner_share_bps`.

## Facility directly

`facility_aggregator.swap(payer, recipient, token_in, token_out, amount_in, min_out, facility)`
trades with one facility without the router. It supports sells (RWA → base),
buys (base → RWA) and RWA → RWA. The protocol fee is taken in the base asset, on
the output of a sale or the input of a buy; RWA → RWA pays none.

## Errors you will see

| Contract | Error | Fix |
|---|---|---|
| router | `SourceUnderDelivered` | a leg paid less than its `min_maker_amount`; requote |
| router | `BelowMinOut` | the route total is under `min_out` |
| router | `InputShortfall` / `OverSpent` | legs' `taker_amount`s do not add up to what was pulled; rebuild the route |
| router | `LegTakerMismatch` / `LegSignatureMismatch` | a quoted RFQ bid names another taker, or its taker signature is wrong |
| any token | `Error(Contract, #1)` / allowance errors | approve the router or settlement (table above) |
| facility | `NoPrice` / `CapExceeded` / `InsufficientLiquidity` | the facility cannot take this trade now; drop the leg |
| Stellar classic | `trustline entry is missing` | open a trustline for the asset you receive |
