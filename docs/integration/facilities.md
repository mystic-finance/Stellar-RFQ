# Facilities: curators and depositors

A **facility** is a curated pool of stablecoin that buys tokenised RWA from
sellers at a known discount, and sells it back. Depositors hold **oLP**, a share
token whose value is the facility's NAV. The facility is an orchestrator only:

- a **vault** per asset holds the tokens; one base vault holds the stablecoin,
  and one vault per RWA holds that RWA;
- every vault can lend its idle balance to **venues** (Blend v2, DeFindex, any
  SEP-56 vault) through allowlisted adapters;
- tokens never pass through the facility, the router or an aggregator. The
  seller's RWA goes straight into its vault, and the payout comes straight out
  of the base vault.

Everything below uses the SDK (see [sdk.md](sdk.md)); each call maps 1:1 to a
contract function of the same name.

## Roles

| Role | Set in | Can |
|---|---|---|
| Protocol admin | facility aggregator | allowlist adapter code, activate facilities for routing, set the protocol fee, `write_down` a failed redemption |
| Curator | `FacilityConfig.curator`, `VaultConfig.curator` | submit and accept timelocked changes, set the allowlist, pause, delist an RWA |
| Manager | `.manager` | allocate and deallocate vault funds, book and settle redemptions |
| Guardian | `.guardian` | revoke a pending change; pause |

## 1. Create the vaults and the facility

```ts
const agg = curator.facilityAggregator();
const me = curatorAddress;
const vaultConfig = (asset: string, symbol: string) => ({
  asset, curator: me, manager: me, guardian: me, name: symbol, symbol,
  deposit_cap: 0n,        // 0 = no cap
  curator_fee_bps: 0,     // performance fee above the high-water mark
  timelock: 172_800n,     // seconds a change waits before it can be accepted
});

const usdcVault = (await (await agg.create_vault({ config: vaultConfig(USDC, "oUSDC") })).signAndSend()).result;
const rwaVault  = (await (await agg.create_vault({ config: vaultConfig(RWA,  "oRWA")  })).signAndSend()).result;

const facility = (await (await agg.create_facility({ config: {
  base_vault: usdcVault, curator: me, manager: me, guardian: me,
  name: "Octarine LP", symbol: "oLP",
  curator_fee_bps: 1000,   // 10% of gains
  vest_seconds: 86_400n,   // a fill's discount reaches NAV linearly over this
  timelock: 172_800n,
} })).signAndSend()).result;
```

Ask the RWA issuer to allowlist **the RWA vault's address**. That address holds
the RWA; the facility never does.

## 2. Connect venues

A venue is enabled through the vault's timelock. The facility aggregator deploys
the adapter instance, and only from code the protocol admin has allowlisted. The
`wasmHashes` for each network are in [../DEPLOYMENTS.md](../DEPLOYMENTS.md).

```ts
const v = curator.vault(usdcVault);
await (await v.submit({ change: { tag: "Venue", values: [Buffer.from(BLEND_ADAPTER_HASH, "hex"), BLEND_POOL] } })).signAndSend();
// …after `timelock` seconds:
await (await v.accept({ kind: 2 })).signAndSend();
const [blendAdapter] = (await v.venues()).result;

// Manager: put idle cash to work, and set the order exits pull in.
await (await manager.vault(usdcVault).allocate({ adapter: blendAdapter, amount: 1_000_0000000n })).signAndSend();
await (await manager.vault(usdcVault).set_venue_order({ order: [blendAdapter, defindexAdapter] })).signAndSend();
```

Change kinds for a vault are `0` curator fee, `1` deposit cap and `2` venue. A
vault holds up to 8 venues. A withdrawal that idle cash cannot cover pulls from
venues in the manager's order. If a venue returns short, the exit reverts; it
never underpays.

## 3. List an RWA

```ts
const terms = {
  accepted: true,
  bid_rate: 1000,         // buys at 10.00 bps/day × horizon
  ask_rate: 500,          // sells at 5.00 bps/day × horizon (≤ bid_rate)
  max_bps: 5000,          // this RWA may be at most 50% of NAV
  oracle: PUSH_ORACLE,    // get_price(rwa, base); see push-oracle.md
  max_price_age: 172_800n,
  redemption: ISSUER_REDEMPTION_ACCOUNT,
  schedule: { mode: { tag: "Rolling", values: undefined }, rolling_seconds: 2_592_000,
              next_redemption_at: 0n, cycle_seconds: 0, max_bps_per_day: 2000 },
};
const f = curator.facility(facility);
const [kind, readyAt] = (await (await f.submit({ change: { tag: "Rwa", values: [RWA, rwaVault, terms] } })).signAndSend()).result;
// …after readyAt:
await (await f.accept({ kind })).signAndSend();
// Protocol admin: route RFQs to it.
await (await admin.facilityAggregator().set_active({ facility, active: true })).signAndSend();
```

The **horizon** is the time to the RWA's next redemption, taken from `schedule`.
`Rolling` means a fixed `rolling_seconds` from now. `Cyclical` means the next
date on a `cycle_seconds` grid that starts at `next_redemption_at`.

| Trade | Output |
|---|---|
| Sell RWA to the facility | `units × price × (1 − bid_rate × h)` |
| Buy RWA from the facility | `value / (price × (1 − ask_rate × h))` |
| RWA → RWA | sell at the bid, then buy at the ask |

`sellQuote`/`buyQuote` in the SDK reproduce this to the unit. A fill never moves
NAV at the moment it happens: the discount is booked as **locked gain** and vests
linearly over `vest_seconds`. A sale below the mark nets against the locked gain
first.

The facility refuses to trade an RWA if any of these holds:

- the RWA is not `accepted`;
- the oracle reading is older than `max_price_age`;
- the trade would push the RWA above `max_bps` of NAV;
- the facility is paused.

In each case `quote` returns 0.

Facility change kinds:

- `0`: curator fee
- `1`: vest seconds
- `2`: the queue
- each RWA has its own kind; `submit` returns it.

The guardian can `revoke(kind, guardian)` any pending change.

## 4. Redemptions

```ts
// Manager: send held RWA to the issuer. NAV carries it as a receivable at the oracle value.
await (await manager.facility(facility).book({ rwa: RWA, amount: units })).signAndSend();
// When the issuer pays, the manager settles. The proceeds are pulled from the manager into the base vault.
await (await manager.facility(facility).settle({ rwa: RWA, proceeds })).signAndSend();
```

- **One open redemption per RWA.**
- **`settle` is all-or-nothing.** Proceeds below the carried value revert, and
  any surplus over the carried value vests as gain.
- **`write_down` (protocol admin) covers an issuer that will not pay in full.**
  It closes the redemption at `recovered < carried`, and the locked gain absorbs
  the loss first.

## 5. Depositors (oLP)

```ts
const f = lp.facility(facility);
await (await f.deposit({ assets: 1_000_0000000n, receiver: me, from: me, operator: me })).signAndSend();
const nav  = (await f.total_assets()).result;
const mine = (await f.convert_to_assets({ shares: (await f.balance({ id: me })).result })).result;
const now  = (await f.max_withdraw({ owner: me })).result;   // what can leave right now
await (await f.withdraw({ assets: now, receiver: me, owner: me, operator: me })).signAndSend();
```

oLP is a SEP-41 token with 6 extra decimals (virtual-share offset), so a
facility over 7-decimal USDC issues 13-decimal oLP. When a curator enables the
allowlist (`set_allowlist`), only members may receive oLP.

### The withdrawal queue

If cash is committed to RWA, `max_withdraw` is lower than your position. Queue
the rest instead:

```ts
const q = lp.queue(QUEUE);
const id = (await (await q.request({ owner: me, receiver: me, shares })).signAndSend()).result;
// Anyone can serve the queue as cash returns (redemptions settle, venues pay back):
await (await anyone.queue(QUEUE).process({ max: 10 })).signAndSend();
// Or take back unserved shares:
await (await q.cancel({ id })).signAndSend();
```

The queue is first come, first served:

- Positions are served at the NAV when served.
- A head position that can only be part-filled keeps its place.
- Queued shares keep their exposure until they are served.
- The facility reserves the queue's claim against every other exit.

The curator connects a queue with `create_queue` on the aggregator, then
`submit({tag: "Queue"})` and `accept` on the facility.

## 6. Pause

`set_paused(caller, true)` (curator or guardian, no timelock) stops all trading:
`quote` returns 0 and `trade` reverts with `Paused`. Deposits, withdrawals, the
queue, redemptions and venue moves keep working, so depositors can always leave.
The protocol admin can also take a facility off the router with
`set_active(facility, false)`, or stop the whole aggregator with `set_paused`.

## Errors

| Code | Facility error | Meaning |
|---|---|---|
| 1 | InvalidAmount | zero/negative, or a share amount that rounds to nothing |
| 2 | InvalidConfig | bad terms (ask > bid, fee too high, wrong vault asset), or caller not curator/guardian |
| 3 | UnsupportedPair | the token is not listed |
| 4 | NotAccepted | listed but `accepted: false` |
| 5 | NoPrice | oracle missing, zero or older than `max_price_age` |
| 6 | CapExceeded | the trade would breach `max_bps` |
| 7 | InsufficientLiquidity | not enough free cash, or not enough RWA to sell |
| 8 | BelowMinOut | output under `min_out` |
| 9 | NotAllowlisted | receiver not a member while the allowlist is on |
| 10 | TooManyRwas | 8 RWAs per facility |
| 11 | StillHeld | delisting an RWA the facility still holds or has out for redemption |
| 12 / 13 / 14 | RedemptionOpen / NoRedemption / NotFullyRepaid | redemption lifecycle |
| 15 | Paused | trading is paused |
| 100–105 | share errors | balances, allowances, frozen holders |
| 110–112 | timelock | nothing pending, not ready yet, caller is not the guardian |
