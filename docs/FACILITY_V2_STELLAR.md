# Facility v2 on Stellar — design

Port of octarine-v2's facility architecture to Soroban, with Octarine's duration
pricing. Replaces the earlier single-contract facility (a vault with a liquidity mode,
plus per-RWA buckets). Minimal LOC is a hard requirement: audit is priced
by lines.

## Decisions (from John)

| Topic | Decision |
|---|---|
| Structure | **Facility = orchestrator, vaults = holder + allocator.** The facility issues **oLP** and holds the vaults' shares |
| Vault | **One asset-agnostic SEP-56 vault contract** used for the stablecoin and for every RWA. It issues shares, holds its asset (so the issuer allowlists the vault for KYC; this replaces buckets), and allocates idle funds to venue adapters (Blend, DeFindex, SEP-56) |
| Pricing | **bps/day × horizon**, in every trade direction |
| NAV | RWA **marked at the oracle**, and the fill discount **vests linearly** to LPs |
| Trades | RWA→stable, stable→RWA, RWA→RWA, capped by allocations (bps) |
| Queue | **First come, first served** withdrawal queue. v2's epochs and bidding are dropped (they were an experiment) |
| Governance | 48h submit → accept timelocks; a guardian can revoke |
| Aggregators | Facility aggregator and DEX aggregator are **standalone**. The DEX aggregator works like **ParaSwap SimpleSwap**: it executes off-chain-generated calldata routes and quotes/swaps through connected DEX adapters, taking a fee in both modes. The Octarine router routes between RFQ settlement and the two aggregators |

## Contracts

```
router ──▶ rfq settlement │ dex_aggregator │ facility_aggregator (factory + registry)
                                                   └─▶ facility (oLP)
                                                         ├─ vault(USDC)  ─▶ adapters
                                                         ├─ vault(RWA A) ─▶ adapters
                                                         └─ queue
```

### `vault`: one asset, any asset
- **Shares:** SEP-56 deposit/mint/withdraw/redeem with operator semantics, plus a SEP-41 share token. Virtual 10^6 offset; rounding always favours the vault.
- **`total_assets`** = idle + Σ adapter `total_assets`.
- **Adapters:** up to 8. They are deployed by the factory from allowlisted wasm, and the curator chooses which to use. The manager allocates/deallocates and sets the pull order. Withdrawals pull from adapters in that order when idle is short.
- **Controls:**
  - deposit cap and optional depositor allowlist, which matters for permissioned RWA shares;
  - `freeze`/`unfreeze` an address's shares and `seize` (compliance; manager);
  - curator performance fee above a high-water mark, minted as shares.
- **No unannounced credit.** Trades have the vault **pull from the payer under the payer's own signature** (`deposit(…, from = payer, operator = facility)`), so a seller's RWA goes straight into its vault without the facility, router or aggregator holding it.

### `facility`: orchestrator and oLP
- **oLP:** SEP-56 over the USDC vault. LP deposits go into the USDC vault; the facility holds the vault shares.
- **NAV** = USDC-vault share value
  + Σ (RWA-vault share value × oracle price)
  + receivables (RWA out for redemption, at oracle or a snapshot)
  − locked (unvested) gain.
- **Per-RWA terms** (timelocked): `bid_rate`, `ask_rate` (≤ `bid_rate`), `oracle`, `max_price_age`, `schedule`, `max_bps` (cap on the RWA's share of NAV).
- **Trades** (h = seconds to redemption, rates in hundredths of a bp per day):
  - **sell RWA** (RWA→USDC): `out = q × p × (1 − bid × h)`. The RWA is credited into the RWA vault and USDC is redeemed from the USDC vault.
  - **buy RWA** (USDC→RWA): `out = q / (p × (1 − ask × h))`.
  - **RWA→RWA:** sell at the bid, buy at the ask.
- **Gain vesting:** for a buy, (oracle value − paid) is added to `locked` and unlocks linearly over `vest_seconds`. A new gain re-weights the unlock end. A sale below the oracle mark is netted against `locked` first.
- **Redemption** (manager): `book` redeems RWA-vault shares to the issuer's redemption address and records a receivable. `settle` is all-or-nothing, paid from the manager or from proceeds already arrived. `write_down` is admin-only.
- **Liquidity:** a synchronous exit takes only cash the queue isn't owed.
- **Timelocks:** RWA terms, fees, queue params and vault set use submit → accept. The guardian revokes pending changes.

### `queue`: first come, first served
- Holds oLP the facility cannot pay out now.
- `process(max)` is permissionless. It serves from the head at the NAV when served and pays the receiver directly. A part-filled head keeps its place.
- `cancel` returns unserved shares to their owner.
- The facility reserves the queue's claim against every other exit.

### `facility_aggregator`
- **Factory:** deploys facilities and vaults from pinned wasm, and adapters from allowlisted wasm.
- **Registry:** of facilities.
- **Router source:** `quote_all`, and `fill`, whose `data` is a `FacilityRoute { facility, payer }`. The router moves no tokens for a Facility leg.
- **Standalone `swap(user, …)`:** a direct entry point for users.

### `dex_aggregator`
- **Route mode (existing):** split/hop routes through allowlisted DEX adapters.
- **Calldata mode (new, SimpleSwap):** `Vec<Call{target, fn, args, auth}>` executed as given.
  - Auth entries may only authorise `token_in` and spend at most `amount_in`.
  - The output is measured; `balance ≥ Σ owed` still holds.
- Protocol and partner fees in both modes. Standalone `swap`, plus the router's `fill`.

## Unchanged
`rfq` settlement, `oracle` adapter, DEX adapters, venue adapters. `router`: Facility legs now move nothing; the facility pulls from the payer named in the route.

## Status (built, deployed on testnet)

LOC is the count of non-blank, non-comment lines in `src/`, excluding tests.

| Contract | LOC | Tests |
|---|---|---|
| `vault` | 428 | 10 |
| `facility` | 658 | 11 |
| `withdrawal_queue` | 146 | 4 |
| `facility_aggregator` | 406 | 7 (real wasm, including a router end-to-end test) |
| `dex_aggregator` | 598 | 19 (4 calldata, real auth) |
| `push_oracle` | 117 | 3 |
| `shares` (shared) | 302 | via vault and facility |

- **Fees.** The protocol fee is in the base asset; RWA ↔ RWA pays none.
- **Redemptions.** A booked redemption carries a snapshot value (octarine-v2's
  "static" mode).
- **Pause.** A curator or guardian can pause trading without a timelock;
  exits keep working.
- **Accrual.** Every accrual writes the fee and high-water mark
  unconditionally. Venue yield moves NAV between simulation and execution, and
  a conditional write would fall outside the simulated footprint. This was
  found on testnet against DeFindex.

Addresses and the testnet verification run: [DEPLOYMENTS.md](DEPLOYMENTS.md).
