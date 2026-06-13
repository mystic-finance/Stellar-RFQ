# Octarine
## Technical Architecture — Production Build on Stellar

---

# 1. Introduction

## 1.1 High-Level Overview

Octarine is a protocol enabling instant liquidity for RWAs, by auctioning liquidations with institutional LPs, third-party protocols and curated liquidity facilities.

RWAs struggle to get DEX liquidity because they're fragmented, regulated and tend to have built-in redemption times. These are not instant, however, which means that RWAs can't be onboarded in lending, as DeFi liquidations need instant liquidity. It also doesn't allow for managing leveraged loops, as users can't instantly unwind their positions. To top it all off, the absence of instant liquidity makes RWAs less attractive for prospective LPs, who can't afford to have their capital locked up unwillingly. This ultimately means that RWAs can have no utility in DeFi nor any functional secondary markets until instant liquidity is solved, as they can't be traded or otherwise onboarded as collateral.

Octarine changes that, by giving liquidity to RWAs via auctions with LPs, third-party protocols and curated liquidity facilities. When there is a liquidation, either on Octarine or on a connected venue, Octarine detects it and auctions it with connected bidders. We then award the trade to the best bid available. Bids can come from either LPs bidding on the API or from third-party DeFi protocols, thus ensuring the RFQ always has the best price in the market. Bidders on Octarine can at any time create and curate vaults with their bidding strategy, thus making the trade accessible to a broader audience. These vaults keep user deposits in lending strategies and call them when they win a liquidation to send to the user. In return, they then receive and redeem the RWA whilst earning a haircut in the process.

The protocol is thus comprised of several elements, namely:
- A settlement contract, which settles transactions between the auction winner and the user. Supports both swaps and lending market liquidations.
- A backend, which handles and coordinates the auction logic and aggregates bids from both off-chain and on-chain sources. The backend is connected to an API/SDK, such that third-parties can easily bid on the RFQ and integrate with us to enable instant liquidity on their assets.
- A liquidity facility contract, which enables bidders to curate vaults that keep user deposits in lending markets and bid on liquidations with lending TVL;
- Adapter contracts for each protocol that we integrate with. For example, each lending market connected to a facility will need its own adapter contract.
- An RFQ router contract, which routes to the winning bid and settles with that bidder.

Serves this document to outline the architecture that Octarine is implementing to have this protocol on Stellar. In addition to the architecture design for the whole build, this repo has the codebase of a bare-bones settlement contract on Stellar that our team has already made, the first and most crucial piece of the above 5 that make up the full planned build of Octarine on Stellar.

How this settlement contract works is simple: a user submits a swap request, the platform runs a short auction amongst connected bidders and then gives the trade to the winning bid. The trade is then settled in a single atomic transaction, exchanging each party’s assets with one another whilst taking a protocol fee. This is live on Stellar Testnet already, deployed with the contract CAPVBMQBVQVDFDWFGH4M3EJH7CYM7MWIYE5TOYTYASOU26L2Q4T2YJZW. Let us now look into the architecture of the whole protocol in more detail, the settlement contract included.


## 1.2 Definitions, acronyms and abbreviations

- **RFQ** – Request-for-Quote: an auction sale of an asset by a maker, who creates
  it via a signed quote (off-chain LP) or a live on-chain quote (facility / DeFi
  source) that a taker fills on-chain.
- **Maker** – The party providing liquidity (e.g. the LP paying stablecoin for an RWA). 
  May be an off-chain signer or an on-chain source.
- **Taker** – The party filling the order on-chain (gives the taker token — e.g.
  the RWA holder seeking instant liquidity). Also called the **seller**.
- **PMM** – Principal Market Maker: an institutional LP that bids on the RFQ with
  its own balance sheet (e.g. GSR, Auros).
- **RFQ Router** – The on-chain contract that aggregates every bid source, picks the bid with the
  best price, and settles the winning route atomically.
- **DEX aggregator** – The on-chain component that brings public DEX liquidity
  into the RFQ as a bid source, quoting and routing across integrated DEXes.
- **Facility aggregator** – The on-chain component that collects and ranks bids
  across all curated liquidity facilities and routes the winning facility's fill.
- **Facility** – A curated, share-based vault that keeps
  depositor funds in yield strategies and bids on the RFQ with that TVL.
- **Curator** – The party that creates and curates a facility and runs the  strategy.
- **Venue** – An external Stellar DeFi protocol a facility deploys capital into
  (e.g. a lending market or a vaults product).
- **Adapter** – A thin contract giving a facility a uniform interface over one
  external venue, so capital can be deployed and pulled without venue-specific code.
- **Haircut** – The discount a bidder pays below redemption value, i.e. the spread for providing instant liquidity.
- **NAV / share** – Net asset value of a facility and the unit of depositor
  ownership; share price = NAV / shares outstanding.
- **Base assets** – The assets a facility deposits and pays out in
  (typically a stablecoin, e.g. USDC).
- **Liquidation** – Forced sale of RWA collateral on a connected lending venue when
  a position becomes unhealthy; a primary source of RFQ flow.
- **Soroban** – Stellar's smart-contract platform.
- **SEP-41** – Stellar's standard token interface (`approve`, `transfer_from`,
  `balance`).
- **SEP-53** – Stellar's message-signing standard; the analogue of EVM's EIP-712.
- **SAC** – Stellar Asset Contract: the Soroban wrapper exposing a classic Stellar
  asset through the SEP-41 token interface.
- **strkey** – Stellar's address encoding (`G…` accounts, `C…` contracts).
- **Allowance** – Permission to spend a token up to an amount (carries an
  `expiration_ledger`).
- **Fee recipient** – The address that receives the protocol fee.
- **XDR** – Stellar's canonical binary serialization (used for order hashing).
- **Soroban RPC** – The JSON-RPC endpoint for simulating and submitting Soroban
  transactions.

---

# 2. Architecture Overview & Constraints

## 2.1 The protocol at a glance

Octarine provides **off-chain auctions with on-chain settlement**. The
backend never holds keys or funds; it coordinates a short auction and gives the
trade to the winning bidder for on-chain settlement.

Three classes of liquidity compete on every trade:

| Liquidity source | How it bids | How it settles |
|---|---|---|
| **Off-chain LP** | `POST /bid` with a SEP-53-signed maker order | Settlement contract `fill_*` (signed maker leg) |
| **Curated facility** | On-chain `quote` from facility aggregator | Router swaps using facility liquidity |
| **Third-party DEXes** | On-chain `quote` from DEX aggregator | Router swaps using DEX liquidity |

The backend fetches and ranks all three, the bid with the **best price** wins, and the **RFQ router**
settles the winning route, in one atomic transaction.

## 2.2 High-Level Architecture

```
        Taker                                           Off-chain LP
   holds an RWA, wants instant                   quotes & SEP-53-signs maker
   liquidity                                      orders (either through a bot or through the UI)
            │  submits request                              │  POST /bid (signed)
            │                                               │
            ▼                                               ▼
   ╔═══════════════════════════════════════════════════════════════════════════╗
   ║                          OCTARINE  PROTOCOL                               ║
   ║   Off-chain auction over on-chain atomic settlement for RWA liquidity.    ║
   ╚══╤═════════════════╤═════════════════╤═════════════════╤══════════════════╝
      │                 │                 │                 │
      │ deposit /       │ curate /        │ token           │ compliance
      │ withdraw        │ set strategy    │ transfers       │ 
      ▼                 ▼                 ▼                 ▼
  Facility          Curator           SEP-41 / SAC      KYC + regulated-asset permissioning
  depositors                         token contracts   
  (liquidity providers)              (RWA / stable)    
            │                                   ▲
            │                                   │  call venue liquidity for redemptions & liquidations
            ▼                                   │
  Venues (lending markets, vaults) ─────────────┘
```

## 2.3 Zoom into the Octarine System (Component diagram)

The off-chain platform runs the auction and hands the taker wallet-signable
operations; the on-chain contracts settle the winning route. On-chain, the **RFQ
Router** is the single front door: it draws bids from three sources — the
**Settlement contract** (off-chain LP signed orders), the **DEX Aggregator**
(DEX liquidity) and the **Facility Aggregator** (curated vault bids) and
picks the bid with the best price, and settles atomically. Facilities reach their yield venues
through **Adapters**.

```
   Sellers                Off-chain LPs               Depositors / Curators
   swap / redeem          POST /bid (signed)          deposit · curate
        │                       │                            │
        └───────────────┬───────┴──────────────┬─────────────┘
                        ▼                      ▼
  ╔════════════════════════════════════════════════════════════════════════════╗
  ║  OCTARINE PLATFORM   (off-chain, keyless — holds no funds and no keys)     ║
  ║                                                                            ║
  ║  ┌────────────────┐   ┌───────────────────────────┐    ┌────────────────┐  ║
  ║  │ Frontend       │──▶│ Backend (NestJS)           │──▶│ MongoDB        │  ║
  ║  │ React + Wallets│◀──│ auction · bid intake ·     │◀──│ requests·bids· │  ║
  ║  │ Kit            │   │ quote aggregation ·        │   │ facilities·    │  ║
  ║  └────────────────┘   │ keyless op assembly ·      │   │ registry       │  ║
  ║                       │ API/SDK · keepers          │   └────────────────┘  ║
  ║                       └─────────────┬──────────────┘                       ║
  ╚═════════════════════════════════════╪══════════════════════════════════════╝
                                         │  wallet-signed invoke + read-only simulate
                                         ▼
  ╔═══════════════════════════════════════════════════════════════════════════╗
  ║  STELLAR / SOROBAN   (the on-chain components)                            ║
  ║                                                                           ║
  ║                    ┌────────────────────────────────┐                     ║
  ║                    │           RFQ Router           │                     ║
  ║                    │  aggregate bids · best price · │                     ║
  ║                    │  atomic fill · protocol fee    │                     ║
  ║                    └──┬───────────────┬──────────┬──┘                     ║
  ║        signed bids    │               │          │   on-chain bids        ║
  ║      ┌────────────────┘       ┌───────┘          └────────┐               ║
  ║      ▼                        ▼                           ▼               ║
  ║ ┌──────────────┐    ┌───────────────────┐    ┌────────────────────────┐   ║
  ║ │ Settlement   │    │ DEX Aggregator    │    │ Facility Aggregator    │   ║
  ║ │ Contract     │    │ best DEX path     │    │ best facility bid      │   ║
  ║ │ (LP signed   │    └─────────┬─────────┘    └───────────┬────────────┘   ║
  ║ │  orders)     │              │                          │                ║
  ║ └──────┬───────┘              ▼                          ▼                ║
  ║        │            ┌───────────────────┐    ┌────────────────────────┐   ║
  ║        │            │ DEXes             │    │ Liquidity Facilities   │   ║
  ║        │            │ Soroswap·Aquarius │    │ (curated vaults)       │   ║
  ║        │            └───────────────────┘    └───────────┬────────────┘   ║
  ║        │                                                 │                ║
  ║        │                                                 ▼                ║
  ║        │                                     ┌────────────────────────┐   ║
  ║        │                                     │ Adapters (per venue)   │   ║
  ║        │                                     └───────────┬────────────┘   ║
  ║        │                                                 ▼                ║
  ║        │                                     ┌────────────────────────┐   ║
  ║        │                                     │ External venues        │   ║
  ║        ▼                                     │ lending markets·vaults │   ║
  ║ ┌──────────────────────────────────┐         │ (yield + redemption)   │   ║
  ║ │ SEP-41 / SAC tokens (RWA·stable) │◀────────┤                        │   ║
  ║ └──────────────────────────────────┘         └────────────────────────┘   ║
  ╚═══════════════════════════════════════════════════════════════════════════╝
```

The on-chain components are the Soroban contracts (router, settlement, the two
aggregators, facilities and adapters) plus the SEP-41/SAC tokens they move.
Everything above the chain is off-chain.

## 2.4 Architecture constraints

- **Non-custodial backend** — every value-changing action is signed by
  a wallet (taker, LP, depositor) or authorised by a contract under its on-chain
  policy. The backend holds no keys and no funds.
- **Atomic settlement** — all legs of a fill (token swap, protocol fee, and any
  venue liquidity pull) move in one transaction or the whole fill reverts. No
  partial settlement state, including across blended multi-source routes.
- **Best-price execution** — the router selects the best bid (or blend
  of bids) and enforces a taker specified minimum output; the fill reverts if the
  taker would receive less than quoted.
- **Many bid channels, one auction** — off-chain signed orders, DEX liquidity and
  facility bids are ranked together; all settle through the same atomic transaction.
- **Signatures produced by wallets** — maker orders must be signable
  by browser wallets (xBull/Freighter) or bots wallets, using the same scheme
  (SEP-53). Contract sources (DEXes, facilities) bid via on-chain quotes, not
  signatures.
- **Replay safety** — signatures are bound to a specific deployment (domain
  separation) and network (SEP-53 passphrase).
- **Token model** — assets follow **SEP-41**, allowances are explicit and expire. 
  Regulated RWAs may additionally enforce transfer authorization at the token level.
- **Curated facilities** — facilities act only within their
  curator-set policy; they never take discretionary action outside it.
- **Modular venue integration** — facilities reach external venues only through
  adapters implementing a fixed interface; adding a protocol means adding an
  adapter, not changing core contracts.
- **Networks** — Stellar **testnet** and **mainnet** only; all deployments via
  `stellar-cli`.


# 3. Contract Overview

The protocol's on-chain logic is split across the Soroban contracts below, each
given with its key functions.

## 3.1 Settlement Contract

A Soroban contract that settles maker-signed orders between two SEP-41 tokens. It
verifies the maker's signature, computes the proportional fill, atomically swaps
the two legs via `transfer_from`, and skims the protocol fee from the maker's
output. It is the settlement core for off-chain (LP / PMM) bids, and the
signed-bids leg beneath the router.

**Key Functions:**

- **SAC allowance** → makers and takers must grant the contract a SEP-41/SAC
  allowance before it can pull funds from their wallets; custody never leaves the
  wallet and the backend holds no keys.
- **Fill (`fill_rfq_order` / `fill_limit_order`)** → the taker submits a
  maker-signed order; the contract checks the order is still fillable, verifies the
  SEP-53 signature, clamps the fill to the remaining amount, and settles.
  `fill_or_kill_*` variants require an exact fill or revert.
- **Settlement math** → `taker_filled = min(fill, taker_amount − filled)`;
  `maker_filled = floor(taker_filled × maker_amount / taker_amount)`;
  `fee = floor(maker_filled × token_fee_amount / maker_amount)` (256-bit
  intermediates avoid `i128` overflow). Taker receives `maker_filled - fee`.
- **Signature verification (SEP-53)** → the maker signs the order hash as a SEP-53
  message; the contract recomputes `SHA256("Stellar Signed Message:\n" ‖
  order_hash)` and `ed25519_verify`s it. A maker signing its own order needs no
  registration (its ed25519 key is recovered from its `G…` address); delegated hot
  keys are authorised via `register_order_signer`.
- **Cancellation** → `cancel_{rfq,limit}_order` (single) and `cancel_pair_*`
  (invalidate all of a maker's orders for a pair below a salt).
- **Fees** → limit orders carry `token_fee_amount` → `fee_recipient`;.
- **Admin** → `initialize(admin)` and native `upgrade(wasm_hash)`.

## 3.2 RFQ Router

A Soroban contract that aggregates every bid source for a request, selects the best
execution, and settles the winning route atomically against a taker minimum output. It composes of the settlement contract for signed bids and the DEX and facility
aggregators for on-chain liquidity, so one trade can settle against a
single source or a blend of several.

**Key Functions:**

- **Quote aggregation (`quote`)** → polls each aggregator for a price (onchain bid)
  at the trade size and returns the list; read-only, used by the backend
  to get on-chain bids for the auction.
- **Route & fill (`fill`)** → the taker submits the chosen route;
  the router executes each leg, sums the taker's realised output, and asserts it
  meets `min_out`, reverting the whole transaction otherwise.
- **Source registry (`register_source`)** → governance whitelists the settlement
  contract, DEX aggregator and facility aggregator as routable sources.
- **Atomicity & fees** → every leg settles in one transaction; signed legs inherit
  the settlement contract's submission gating with the router as the authorised
  origin, and a protocol fee is skimmed from the settled output.
- **Admin** → `initialize(admin, fee_recipient, fee)` and native `upgrade(wasm_hash)`.

## 3.3 DEX Aggregator

A Soroban contract that brings Stellar's on-chain liquidity from DEXes as a liquidity source. It quotes the best path across
integrated DEXes for a given size and executes that swap on the router's behalf.

**Key Functions:**

- **Quote (`quote`)** → returns the best obtainable output across integrated DEXes
  for `(token_in, token_out, amount)`; read-only.
- **Swap (`swap`)** → executes the quoted path under the router's call, reverting
  if the path can't deliver.
- **DEX registry** → admin registers the DEXes the aggregator routes through (e.g.
  Soroswap, Aquarius, Phoenix).

## 3.4 Facility Aggregator

A Soroban contract that collects and ranks quotes across all curated facilities for a
requested RWA and settles the winning facility's fill. It is the single integration
point the router sees for the whole facility ecosystem.

**Key Functions:**

- **Facility registry (`register_facility`)** → curators register a facility and the
  assets it serves; governance can pause or revoke it.
- **Quote (`quote`)** → polls each eligible facility's bid price for the RWA and
  size and returns the ranked set; read-only.
- **Route & fill (`fill`)** → forwards the winning facility's fill request under the
  router's call.


## 3.5 Liquidity Facility

A Soroban contract implementing a curated, share-based vault that keeps depositor
funds in yield venues and bids on the RFQ with that TVL. On winning it pulls
liquidity from its venues, pays the taker, takes the RWA, and later redeems it for
a haircut that accrues to share value net of a curator fee.

**Key Functions:**

- **Deposit / withdraw (`deposit` / `withdraw`)** → depositors mint shares at the
  current NAV and burn them to redeem for stablecoins; withdrawals are served up to
  free liquidity and otherwise queued until redemptions settle.
- **NAV & shares** → `NAV = idle_base + venue_balances (incl. accrued yield) +
  acquired_RWA (held at cost)`; `share_price = NAV / shares`.
- **Bid (`quote`)** → returns the facility's price for an RWA redemption for a given amount within
  its curator-set caps.
- **Redeem assets for stablecoins (`redeem_for_assets`)** → called by the aggregator on a win:
  validates price and caps, pulls just enough stablecoins from venues via adapters, pays
  the seltakerler, takes the RWA, and books it for redemption, inside the
  router's atomic fill.
- **Venue allocation (`allocate` / `deallocate`)** → idle stablecoins are deployed to
  whitelisted venues and pulled back on demand, bounded by each adapter's
  withdrawable balance.
- **Redemption (`book_redemption` / `settle_redemption`)** → acquired RWA is redeemed
  with the issuer (T+N) by the facility manager;.

## 3.6 Adapters

Thin Soroban contracts that give a facility a uniform interface over one external
venue, so assets can be deployed and pulled. Adding a
protocol to the ecosystem means writing and whitelisting one adapter, never
touching the facility, aggregator, or router code.

**Key Functions:**

- **Deposit / withdraw (`deposit` / `withdraw`)** → move the stablecoins between the
  facility and the venue, returning the actual amount moved.
- **Balances (`total_assets` / `max_withdraw`)** → report the facility's current
  redeemable balance (including accrued yield) and how much can be withdrawn
  instantly; the latter bounds how much a facility can safely bid.
- **Scope** → two adapters ship first: a lending market and a vaults product; DEX
  liquidity is integrated under the DEX Aggregator.

---

# 4. Protocol Flows

## 4.1 Off-chain LP wins a swap (direct settlement)

When a single off-chain maker wins, the taker fills the settlement contract
directly through the rfq router.

```
 Taker              Backend            LP(maker)          Router + Settlement
   │ POST /swap        │                   │                    │
   ├──────────────────▶│ create request    │                    │
   │                   │   POST /bid  ◀─────┤ build + sign order│
   │                   │  verify SEP-53     │ approve(stable)   │ approve()
   │ best bid + ops ◀──┤  assemble ops      │                   │
   │ sign + submit ────┼───────────────────────────────────────▶ fill_rfq_order
   │  (wallet)         │                    │                   │  verify · swap · fee
   ▼                   ▼                    ▼                   ▼
        RWA: seller→LP                       stable: LP→seller (− fee)
```

## 4.2 On-chain win via the router (DEX / facility)

When DEX or facility liquidity wins, or when the best execution is a blend, the
taker signs a single router call and the router settles every leg atomically.

```
 Taker             Backend          Router                Sources
   │ POST /swap      │                 │                     │
   ├────────────────▶│ open auction    │                     │
   │                 │ quote() ───────▶│ poll ─────────────▶ │ Settlement · DEX Agg · Facility Agg
   │                 │ + POST /bid (LP)│                     │
   │ route + ops ◀───┤ rank → best/blend                     │
   │ approve(RWA)    │                 │                     │
   │ sign router.fill┼────────────────▶│ fill(route,min_out) │
   │                 │                 │  ├─ signed leg ────▶│ Settlement.fill_*
   │                 │                 │  ├─ DEX leg ───────▶│ DEX Agg → DEXes (swap)
   │                 │                 │  └─ facility leg ──▶│ Facility Agg → facility:
   │                 │                 │                     │ pull venue → pay → take RWA
   │                 │                 │  assert out ≥ min_out
   │                 │                 │  skim protocol fee  │
   ▼                 ▼                 ▼                     ▼
   --------------------------------------------------------------------------
   Taker receives ≥ min_out stablecoin (route settle, or the tx reverts)
   --------------------------------------------------------------------------
```

## 4.3 Facility deposit / withdraw

```
 Depositor        Backend          Facility            Adapter
   │ deposit        │                 │                     │
   │ approve(base)  │ assemble op     │                     │
   │ sign deposit ──┼────────────────▶│ mint shares @ NAV   │
   │                │  (curator)─────▶│ allocate idle ────▶ │ deposit()
   │ sign withdraw ─┼────────────────▶│ burn shares;        │
   │                │                 │ deallocate if ────▶ │ withdraw()
   │                │                 │ needed; pay base    │
   ▼                ▼                 ▼                    ▼
   
   (if free liquidity is insufficient → withdrawal queued, served on redemption)
```

## 4.4 Redemption lifecycle

```
 win bid ─▶ hold RWA  ───────────▶ manager redeems with issuer (T+N)
                                              │
                                  issuer settles in stablecoins
                                              │
                                              ▼
                       stablecoins returned to facility → NAV increases → release queued withdrawals (if any)
```

Between winning and settlement the facility carries the RWA at cost and its capital
is temporarily tied up; the yield compensates depositors for that duration and
the redemption-time risk.

## 4.5 Lending-venue liquidation trigger

The core utility unlock: a lending market can accept RWA collateral because
Octarine guarantees an instant buyer at liquidation time.

```
 connected lending venue ──position unhealthy──▶ bots detect ──▶ backend opens auction (LPs · DEX · facilities)
        │                                                                  │
        ▼                                                                  ▼
 collateral RWA seized ──────────────────▶ sold via the RFQ router────▶ stablecoin to repays the loan
```

---

# 5. Technology Stack & Infrastructure

## 5.1 Smart contracts (Soroban)

- **Settlement contract** — RFQ + limit order settlement;
  SEP-53 signatures, SEP-41 settlement.
- **RFQ router** — atomic multi-source settlement + fee.
- **DEX aggregator** — quoting and routing across public Stellar DEXes.
- **Facility aggregator** — quoting and routing across curated facilities.
- **Facility** — share-based vault, redemption accounting.

## 5.2 Backend

- **NestJS (TypeScript)** — auction coordination, bid intake, on-chain quote
  aggregation, keyless Soroban op assembly, keepers; writes to MongoDB.
- **API/SDK** — typed client for LP bidding bots, third-party integrators, and the
  curator console.
- **Soroban reads** – Simulates `get_*_order_hash`, `get_*_order_info`,
  `is_order_signer`, and token `balance` for signature verification, status, and
  pricing.
- **MongoDB** — requests, bids, fills/approvals, facilities, token registry.

## 5.3 Frontend

- **React + Vite (TypeScript)** — swap/redeem, auctions/bid board, LP bid flow,
  facility deposit/withdraw, curator console, dashboards, live balances.
- **Stellar Wallets Kit** — xBull, Freighter (+ Albedo, Rabet, Lobstr, Hana):
  `signTransaction` for fills/router calls, `signMessage` (SEP-53) for maker orders.

## 5.4 Infrastructure

- **Azure VMs + nginx** — Host the NestJS backend and the
  frontend build, with **nginx** as the reverse proxy / TLS terminator in front of
  the API and static assets.
- **MongoDB on Azure** — Order book, bids, approvals, and the token registry,
  hosted on Azure for low‑latency access from the API VMs.
- **Cloudflare Pages** — Static hosting + CDN for the frontend deployment.
- **Soroban RPC / Horizon** — simulate + submit; account/ledger data.
- **Indexing & Monitoring** — indexing fills, facility NAV, allocations,
  and redemptions.
- **stellar-cli pipeline** – Deterministic build → optimize (~21 KB WASM) →
  deploy → `initialize`, with addresses written to `deployments/<network>.json`.

---

# 6. Integrations

- **Stellar Wallets Kit** (xBull, Freighter) — wallet connection + tx/message
  signing.
- **Soroban RPC / Horizon** — simulation, submission, balance/ledger queries.
- **SEP-41 / SAC token contracts** — the RWA and stable assets the protocol settles.
- **DEXes** (Soroswap, Aquarius, Phoenix) — public liquidity for the
  DEX aggregator.
- **Lending markets & vault products** — yield venues behind facility adapters.
- **KYC / compliance provider** — identity checks and regulated-asset gating.

---
