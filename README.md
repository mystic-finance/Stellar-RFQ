# Octarine Settlement — duration-priced RFQ for Soroban

Soroban (Rust) settlement for RWAs whose value is a function of **time to
redemption**. Three order types settle through one contract:

| Order type | Price comes from | Who signs | Custody |
|---|---|---|---|
| **RFQ** | the rate model — `bps/day × horizon` off an oracle price | both sides sign off-chain (SEP-53) | none, allowance-based |
| **Fixed** | the maker states the amount outright | both sides sign off-chain (SEP-53) | none, allowance-based |
| **Dutch** | an on-chain ask decaying from a start price to a floor | nobody — terms are posted on-chain | the contract escrows the seller's asset |

Two more contracts sit around it: the **RFQ router**, which ranks every bid
source for a trade and settles the winning route atomically, and the **oracle
adapter**, which reads a SEP-40 feed (Reflector) and republishes it in the price
convention the settlement contract expects.

The code is intentionally comment-light; the reasoning lives here.

---

## The pricing model

An RWA that redeems at par in *N* days is not worth par today. Whoever buys it
now is financing the gap, and the price of that financing is quoted as a rate —
**basis points per day** — rather than as a discount the maker has to recompute
every time the clock moves.

Each asset carries a **redemption schedule** that the contract turns into a
horizon in seconds, exact to the second:

- **`Rolling`** — always `rolling_seconds` away (a T+N instrument).
- **`Cyclical`** — a calendar anchor (`next_redemption_at`) repeating every
  `cycle_seconds`. Once the anchor passes, the horizon rolls to the next cycle.
  The last hours before a redemption window price as hours, not as a whole day.

A fill is charged over the **net** horizon. If the taker's asset redeems in 30
days and the maker's redeems in 10, only the 20-day gap is financed:

```
horizon      = max(seconds(taker_token) − seconds(maker_token), 1)
discount     = maker_bps_per_day × horizon            # bps·seconds
gross        = taker_amount_in × price(taker_token → maker_token) / 1e18
maker_amount = gross × (10_000·86_400 − discount) / (10_000·86_400)
fee          = maker_amount × fee_bps / 10_000
```

Rates are quoted per day while horizons are counted in seconds, so the
denominator is `BPS × 1 day`. The maker leg needs no schedule (a stablecoin has
none); the taker leg always does, and its schedule also carries the
`max_bps_per_day` ceiling no maker may quote above.

Three separate bounds have to hold on every RFQ fill, so neither side can be
walked past what it agreed to:

- `maker_bps_per_day ≤ schedule.max_bps_per_day` — governance's ceiling.
- `maker_bps_per_day ≤ taker_max_bps_per_day` — the taker's own ceiling.
- `maker_amount ≤ max_maker_amount × slice` and `received ≥ min_received_amount × slice`
  — the maker's ceiling and the taker's floor, both pro-rated to the slice being
  filled, so splitting one fill into many cannot walk past either.

Both bounds round **against** the party they protect (the cap floors, the floor
ceils), which is what makes them un-walkable — and it means a bound signed at
*exactly* the full-fill quote sits on a one-raw-unit knife-edge: a partial fill
can miss it by 1 and revert. Sign bounds with **proportional** slack — a
slippage tolerance in bps, not a fixed number of units, since an absolute
cushion is pro-rated away to nothing on a small slice. A slice too small to
quote a positive output is rejected outright rather than settling for zero.

## Prices

`price_of(base, quote)` is scaled so that

```
quote_amount = base_amount × price / 1e18
```

in **raw token units** — decimals are already folded in, so nothing downstream
has to know them. Resolution order:

1. A feed registered directly for `(base, quote)`.
2. Otherwise `unit(base) / unit(quote)`, where each leg is the asset's price in
   the **reference asset** — a registered feed first, then the admin-pushed
   backstop.

Every feed read goes through `try_invoke`: a missing, trapping, zero or stale
oracle is treated as *no price* and falls through to the next source rather than
bricking settlement. Pushed prices are guarded by a max deviation per update and
a staleness bound, and they are stamped with a **reference epoch** —
repointing the reference asset bumps the epoch and retires every pushed price at
once, so a stale number denominated in the old reference can never be reused.

### The oracle adapter

`contracts/oracle` wraps one SEP-40 feed for one pair:

- `cross: false` — the feed already quotes in the quote asset.
- `cross: true` — two `lastprice` legs are divided (e.g. `RWA/USD ÷ XLM/USD`);
  the returned timestamp is the **older** of the two.
- `invert: true` — the feed reports `quote/base`; the adapter flips it.

It then rescales by `10^(18 + quote_decimals − base_decimals − feed_decimals)`
and traps on a stale or absent price. Any contract exposing
`get_price(base, quote) -> {price, timestamp}` can be registered instead.

## The router

The auction does **not** happen on-chain. A taker's request, the LP bids, the
quotes pulled from the DEX and facility aggregators, the ranking, and the
taker's choice of route all live in the backend. The router's job is narrower
and safer: **execute the route the taker picked, atomically, or revert.**

```
  taker request ─▶ backend ─┬─▶ off-chain LPs         (signed bids)
                            ├─▶ facility aggregator   (on-chain quote)
                            └─▶ DEX aggregator        (on-chain quote)
                                        │
                            backend ranks them, taker picks a route
                                        │
                                        ▼
                            router.fill(route, min_out)   ← one transaction
```

One leg kind per bid channel, matching the choice the taker made:

| Leg | Settles against |
|---|---|
| `Rfq { order, maker_signature, taker_signature, taker_amount }` | an MM's signed bid, straight through the settlement contract |
| `Dex { aggregator, taker_amount, min_maker_amount, data }` | a DEX aggregator |
| `Facility { aggregator, taker_amount, min_maker_amount, data }` | the facility aggregator |

The signed order carries its own pricing type — `SignedOrder::Rfq` (duration-priced)
or `SignedOrder::Fixed` (amount stated outright) — so the leg says *which channel*
and the order says *how it was priced*.

A route is a `Vec<Leg>`. The normal case from the flow above is one leg, but the
list also lets a large request settle across several counterparties at once.

**The router never re-picks a leg.** It has no notion of "best" at fill time —
that decision was made off-chain, with the taker looking at it. `min_maker_amount`
is the payout the taker was *shown* when they chose, and the aggregator is held
to it. `data` is an opaque payload (a DEX path, a facility id) the backend puts
in the route and the router passes through untouched.

`quote(taker_token, maker_token, amount)` is a read-only sweep of registered
aggregators, tagged by kind, so the backend can pull the on-chain side in one
call while it collects the signed book. Registration is kind-aware: a `Dex` leg
cannot be pointed at the facility aggregator.

### How value moves, and which approval that needs

A leg either settles **straight between the counterparties**, or the router pays
for it. Which one applies is decided by a single question — *does the maker's
order name a taker?* — and that answer is what determines which contract the
taker must have approved.

| Leg | `order.taker` | Who moves the input | Taker must approve |
|---|---|---|---|
| `Rfq` — quoted bid | the taker | settlement, from the taker's own allowance | **settlement** |
| `Rfq` — open bid | none | router pulls, then hands it to settlement | **router** |
| `Dex` / `Facility` | n/a | router pulls, then hands it to the aggregator | **router** |

So the rule for a frontend is mechanical: **approve settlement if the route
contains a bid quoted to you; approve the router if it contains an open bid or
any aggregator quote.** A route with both needs both — each a one-time approval,
and the full set is derivable from the chosen route before anything is signed.

```
fill():
  1. validate every leg          ← nothing moves until the whole route is sound
  2. pull the input for the ROUTER-PAID legs only  (one transfer_from)
  3. per leg:
       Rfq, quoted → call settlement; it pulls from the taker and pays them back
       Rfq, open   → approve settlement for this leg; it pulls from the router
                     and pays the router, which is the taker of record
       Dex / Fac   → transfer(router → aggregator); the aggregator pays the router
  4. forward what the router collected, refund anything unspent
  5. measure the TAKER's balance delta and check it against min_out
```

Measuring the taker's own position at the end is what lets the two paths mix: it
counts proceeds that arrived directly from settlement and proceeds the router
forwarded, in one number.

**Why quoted bids pass through.** Two things fall out of not routing them:

- **The bid stays bound to the counterparty it was quoted to.** Settlement pulls
  from and pays that exact address, and the router refuses a leg quoted to a
  third party (`LegTakerMismatch`), so nobody can consume someone else's quote
  by routing it.
- **A permissioned RWA never has to whitelist the router**, since the token only
  moves taker → maker. Only the router-paid legs put it in the transfer graph.

**Why open bids don't need any of that.** An order with no named taker is
liquidity offered to anyone, so there is no counterparty to bind it to and the
router taking it on its own behalf is exactly what the maker signed up for. It
carries no taker signature; a leg whose signature contradicts its order is
rejected (`LegSignatureMismatch`) rather than settling on the wrong path.

### What the router guarantees

Everything is enforced against **measured balances**, never against what a leg
claimed. Because signed legs pay the taker directly and aggregator legs pay the
router, the check is on the taker's own position after the router forwards:

```
amount_out = the taker's maker-token balance delta across the whole route
```

**The router takes no fee.** Each venue skims where it settles — the settlement
contract from the maker's output, the aggregators inside their own quotes — so a
routed trade is charged exactly once and `amount_out` is what the taker keeps.

If `amount_out < min_out` the whole transaction reverts — every leg with it, so a
partially-filled route can never be left behind. Four further guards:

- An aggregator delivering less than the leg's `min_maker_amount` reverts the route.
- A signed leg quoted to anyone but this route's taker is rejected before funds move.
- The taker cannot be made to spend more than the route declared, so a
  misbehaving venue cannot reach past its leg into an allowance granted elsewhere.
- Balances the router already held are excluded from every delta, so stray dust
  can never be swept into a route.

One limitation: a **transfer-taxed input token cannot be routed through an
aggregator leg**, since the legs' amounts are fixed and less input arriving than
declared is unfulfillable; it reverts with `InputShortfall`. Signed legs are
unaffected — settlement prices them on what actually arrived.

## Dutch listings## Dutch listings

The only order type that takes custody. The seller escrows the asset on
`create_dutch_order`; the ask decays linearly from `start_maker_amount` to
`min_maker_amount` over the configured `decay_seconds` and rests on the floor
after that. A buyer names the most they will pay and gets the escrow; the seller
gets the proceeds. `cancel_dutch_order` returns the escrow to the seller.

Escrow is credited by **measured balance delta**, not by the amount requested,
and the listing's start/floor are scaled to what actually arrived. A token that
credits more than it was sent is clamped, so no listing can mint escrow it never
funded.

## Authorisation

Both sides are offline at fill time, so consent is entirely off-chain and
**anyone can submit the transaction**. Three separate things authorise a fill:

**Custody** comes from a pre-existing SEP-41 allowance. Maker and taker each
`approve` the settlement contract as spender; settlement moves both legs with
`transfer_from`. The contract holds nothing outside Dutch escrow.

**The taker signs a request.** Before any maker has bid, the taker signs their
own terms — the pair, the size, their floor, their rate ceiling, the expiry, the
salt, and which settlement path it authorises:

```
Request { maker_token, taker_token, taker_amount, min_received_amount, fee_bps,
          taker, sender, fee_recipient, expiry, salt, taker_max_bps_per_day,
          order_type }
```

Every order repeats those fields, in that order, as its leading half. So **one
taker signature pairs with whichever bid wins** — the maker signs the whole
thing on top, and the request is tracked and filled independently of any single
order. `order_type` is inside the digest, so a signature authorising an RFQ fill
cannot be replayed on the fixed path.

Requiring that second signature is what stops a live order from being a free
option. A maker-signed order naming a taker would otherwise be executable by
anyone, at any moment until expiry, on terms the taker never agreed to.

**The maker signs the order** — the request fields plus its bid.

Both signatures are ed25519 over the SEP-53 digest:

```
order_hash = sha256(DOMAIN || contract_address_xdr || order_xdr)
digest     = sha256("Stellar Signed Message:\n" || order_hash)
```

which is what a wallet's `signMessage` and a bot produce identically. The
contract address in the preimage binds a signature to one deployment. Signers
never re-implement the byte layout — they call the read-only `hash_request` /
`hash_rfq_order` / `hash_fixed_order` and sign what comes back. A signer is
valid if it is the party's own account key, recovered from its `G…` address with
no registration, or a hot key registered via `register_order_signer`.

### Who may submit

`sender` on the request names who may land the fill; `None` means anyone. The
submitter is an explicit parameter — Soroban has no `msg.sender` — and must
authorise the call:

| `taker` | `sender` | Who submits | Taker signature |
|---|---|---|---|
| `None` | `None` | anyone; the submitter **becomes** the taker | not needed |
| named | `None` | anyone — a relayer, the maker, the backend | required |
| named | named | only that address | required |
| named | named, equal to taker | the taker itself | not needed, its own auth says so |

### Cancellation

`cancel_salt(caller, signer, salt)` voids every unfilled order that signer put
under that salt — the taker's request and any maker bid adopting it, since both
sides share the salt. Callable by the signer or by a key they registered, so a
hot key can retract the book it signed. `expiry` still bounds anything never
cancelled, so both sides should sign short windows.

## Roles

Two, plus a pause switch:

- **admin** — config, oracles, reference asset, keepers, pause, upgrade.
- **keeper** — schedules and pushed prices only, and bounded: a keeper's
  schedule change may move neither the current horizon nor the schedule's
  amplitude by more than `max_shift_seconds`. The admin is not bound by it.

---

## Layout

```
contracts/
  rfq/          Settlement: RFQ + fixed + Dutch
    src/
      lib.rs        entry points, settlement, escrow
      price.rs      schedules, the rate model, price resolution
      types.rs      order / schedule / config structs
      hash.rs       domain-separated order hashing + SEP-53 digest
      storage.rs    storage layout & accessors
      errors.rs     contract error codes
      test.rs       unit tests (behaviour, per scenario)
      invariant.rs  property tests (rules that hold over randomised sweeps)
      mock_token.rs test-only token that skews what it credits
  router/       RFQ router: multi-source ranking + atomic route settlement
  oracle/       SEP-40 (Reflector) price adapter
  test_token/   Minimal Soroban token for tests/demos (no trustlines)
crates/
  orders/       Order types shared by settlement + router (never deployed)
scripts/
  lib.sh           shared config (networks, helpers)
  00-setup.sh      generate + fund identities (admin/maker/taker)
  01-build.sh      build + optimise the WASMs
  02-deploy.sh     deploy + initialize  (testnet OR mainnet)
  03-seed-demo.sh  testnet-only: demo tokens, balances, schedule, price
  e2e.sh           run the testnet pipeline end to end
  strkey.mjs       decode a G... address to a raw ed25519 pubkey
deployments/
  <network>.json            deployed addresses (the hand-off artifact)
  accounts.<network>.json   identity addresses + secrets (git-ignored)
```

## Prerequisites

- Rust + the `wasm32v1-none` target: `rustup target add wasm32v1-none`
- [`stellar-cli`](https://developers.stellar.org/docs/tools/cli): `cargo install --locked stellar-cli`
- `jq` and `node` (used by the deploy scripts)

## Build & test

```bash
make build      # cargo build (host)
make test       # unit tests (settlement + oracle)
make wasm       # build + optimise the on-chain WASMs
```

## Deploy

```bash
# Testnet: create + fund identities, then deploy.
make setup
make wasm
make deploy                       # -> deployments/testnet.json

# Mainnet: your own funded deployer and a real reference asset.
NETWORK=mainnet SOURCE=my-deployer REFERENCE=C...usdc make deploy
```

`02-deploy.sh` deploys the contract, calls `initialize(admin, reference)`, and
writes `deployments/<network>.json`. On a test network with no `REFERENCE` set
it first deploys a demo `OUSD` token to act as the reference asset.

Then, testnet only:

```bash
make seed-demo    # ORWA token, balances, maker signing key,
                  # a 30-day rolling schedule and a 1:1 backstop price
make mint TO=G...address
```

Or the whole pipeline: `make e2e`.

---

## Contract API

```
# lifecycle
initialize(admin, reference)
upgrade(new_wasm_hash)                          # admin
get_admin() / get_config() / get_reference() / is_paused()

# governance                                     admin unless noted
set_config(cfg)                                 # fee bounds, staleness, deviation,
                                                # keeper shift limit, Dutch decay
set_reference(asset)                            # bumps the price epoch
set_oracle(base, quote, Some({oracle, max_age}) | None)
set_keeper(keeper, allowed) / set_paused(paused)
set_schedule(caller, asset, schedule)           # admin or keeper
push_price(caller, asset, new_price)            # admin or keeper

# views
seconds_to_redemption(asset) / get_schedule(asset)
price_of(base, quote)
quote_rfq_order(order, taker_amount_in)         # -> {maker_amount, fee, horizon_seconds}
hash_request(request)                           # the taker signs this
hash_rfq_order(order) / hash_fixed_order(order) # the maker signs these
filled_amount(order_hash) / request_filled_amount(request_hash)
is_salt_cancelled(signer, salt)
is_order_signer(maker, signer) / get_listing(id)

# orders
register_order_signer(maker, signer, allowed)   # maker
fill_rfq_order(order, maker_signature, taker_signature, sender, taker_amount_in)
fill_fixed_order(order, maker_signature, taker_signature, sender, taker_amount_in)
cancel_salt(caller, signer, salt)               # signer or a key it registered

# Dutch listings
create_dutch_order(seller, order) -> id
current_ask(id)
fill_dutch_order(id, buyer, max_maker_amount)
cancel_dutch_order(id)                          # seller
```

### Router API

```
initialize(admin, settlement)
upgrade(new_wasm_hash)                                    # admin
register_source(source, kind, allowed)                     # admin; kind = Dex | Facility
set_settlement(settlement) / set_paused(paused)           # admin
admin() / settlement() / is_paused() / sources()

quote(taker_token, maker_token, taker_amount) -> Vec<SourceQuote>
best_quote(taker_token, maker_token, taker_amount) -> Option<SourceQuote>
fill(taker, taker_token, maker_token, route, min_out) -> RouteResult
```

An aggregator (DEX or facility) implements — with the input **already
transferred in** before `fill` is called:

```
quote(token_in, token_out, amount_in) -> amount_out
fill(recipient, token_in, token_out, amount_in, min_amount_out, data) -> delivered
```

### Integration recipe

1. Maker `approve`s this contract for the maker token; taker for the taker token.
2. Maker registers a hot signing key once (skip if signing with its own account
   key): `register_order_signer(maker, pubkey, true)`.
3. Admin/keeper sets a schedule for the RWA and registers a feed (or pushes a
   backstop price).
4. The taker signs their request (`hash_request`); makers bid by signing the
   full order (`hash_rfq_order`) on top. Both are SEP-53 digests.
5. Anyone submits `fill_rfq_order(order, maker_signature, taker_signature,
   sender, amount)` — the taker need not be in the transaction. Simulate
   `quote_rfq_order` first to see exactly what it will pay out, then set
   `min_received_amount` / `max_maker_amount` around it with a bps tolerance — not with
   a fixed number of raw units (see the bounds note above).

To route instead of filling a single bid: the backend collects signed bids and
polls the aggregators, ranks them, and shows the taker the routes; the taker
picks one; the backend assembles it into a `Vec<Leg>` and hands back an unsigned
`fill(taker, taker_token, maker_token, route, min_out)` for the wallet to sign.

---

## Design & security notes

- **Amounts are `i128`** (the SEP-41 amount type); all proportional math uses
  256-bit intermediates, so no product overflows before the divide.
- **Fill state is recorded before funds move**, and every fill re-derives the
  order hash from the struct it was handed — expired, cancelled, salt-invalidated
  and over-filled orders all revert.
- **Received amounts are measured, not assumed.** Both legs compare the
  receiver's balance before and after, clamped to the amount sent, so a
  transfer-taxed or mid-transfer-rebasing token settles for what actually
  arrived and the taker's floor is checked against that number.
- **Signatures are domain-separated by contract address**, so they are not
  replayable across deployments; `salt` is `u64` (Soroban has no native 256-bit
  integer in `contracttype`).
- **Allowances expire.** SEP-41 `approve` carries an `expiration_ledger`;
  makers and takers refresh as needed.
- **Dutch escrow is per listing**, tracked by the amount actually received.
  Nothing pools it across listings, so one listing cannot be released out of
  another's balance.
- **The router takes no custody** and holds no allowance beyond the output-token
  fee skim; a route either settles whole or reverts whole.
- **Order types live in one crate** (`crates/orders`) that both the settlement
  contract and the router depend on, so the two cannot drift on XDR encoding —
  a mismatch there would be a silent mis-settlement rather than a compile error.
- **`test_token` is a throwaway demo token** — never deploy it to mainnet.

## License

Apache-2.0
