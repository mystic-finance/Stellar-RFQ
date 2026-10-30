# SCF Build — Tranche Completion Form (Milestone 2)

## Are you ready to submit your next tranche of deliverables?

Yes.

## Project Stage

Pre-Launch #1 — MVP.

## Telegram Username

@Kelmat

## Tranche Deliverables

**1. Liquidity Facility & Aggregator Smart Contracts.** A facility is a curated
pool of stablecoin that buys tokenised RWA from sellers at a rate per day over
the asset's time to redemption, and sells it back.

- **Share token.** Depositors hold oLP, priced at the facility's NAV.
- **Custody.** One vault per asset holds the funds. The issuer allowlists that
  vault for KYC, so the RWA never sits in the facility or the router.
- **Gains.** A fill's discount vests to LPs linearly.
- **Governance.** Every parameter change waits out a timelock that a guardian
  can veto, and a curator can pause trading instantly without blocking exits.
- **Liquidity.** A first-come, first-served queue serves exits when cash is
  committed to RWA.
- **Facility aggregator.** It deploys facilities and vaults from pinned code,
  and is the router's Facility source.
- **DEX aggregator.** It routes through Soroswap and Aquarius adapters, or
  executes off-chain-built calldata in the style of ParaSwap SimpleSwap.

Done on testnet:

- a facility receives deposits;
- it wins a sale through the router and settles atomically, with the RWA
  going straight into its vault;
- it books the RWA for redemption, then settles the redemption in full.

**2. Venue adapters & RWA pricing.** These are adapters for **Blend v2** and
**DeFindex**, plus a generic **SEP-56** adapter. A vault allocates idle cash to
them and pulls it back during a fill or an exit.

The **push oracle** carries an issuer's NAV on-chain. Privileged updaters push
it, and each push is bounded per step. Facilities enforce staleness: past an
RWA's `max_price_age`, the facility stops quoting that RWA. A TypeScript keeper
reads Centrifuge JAAA's NAV from its ERC-4626 vault on Ethereum and pushes it.

Done on testnet, against the live Blend v2 pool and DeFindex USDC vault:

- the base vault allocates to both;
- a sale is paid partly out of Blend;
- an LP exit pulls back from DeFindex and Blend;
- the facility prices its RWA at the JAAA NAV the keeper pushed.

**3. Curator console & LP SDK.**

- **Curator console.** The Octarine UI has a facility console where curators:
  - create facilities;
  - list RWAs and connect venues;
  - accept or revoke timelocked changes;
  - pause trading;
  - book and settle redemptions;
  - run the queue.

  LPs deposit and withdraw there.
- **SDK.** `@octarine/stellar-sdk` is a TypeScript SDK for frontends and
  backends. It provides typed clients for every contract, router-leg encoders,
  pricing previews, and the auction API client.
- **Example bots.** A market-maker bot bids on RFQs through the auction API.
  A curator operations bot serves the queue, keeps an idle-cash buffer, and
  watches price freshness.
- **Integration docs.** These cover market makers, takers and routing,
  curators, and oracle operators.

**4. Cross-chain RFQ settlement (Stellar ↔ EVM).** A taker on Stellar sells an RWA to a maker on
an EVM chain (or the reverse) in one RFQ fill. There is one settle contract per bridge, and
off-chain picks which to use by token and chains:

- **CCTP** (`cctp_settle` / `CctpSettle`): FILL and FAIL travel as CCTP messages and the payment as
  a CCTP USDC burn. CCTP has no executor, so the second leg is relayed by whoever is paid to: the
  maker's keeper relays FILL, the taker relays FAIL, and anyone `complete`s the payment for a flat
  tip.
- **LayerZero** (`lz_settle` / `LzSettle`): LayerZero messages and OFT payments (USDT0).
- **Delivery modes:** the RWA is delivered on the taker's chain (DELIVER); redeemed through the
  redemption vault into a tradeable claim token (REDEEM_CT); redeemed with the USDC pushed to the
  maker's chain (REDEEM_PUSH); or bridged to the maker's chain where a bridge carries the RWA
  (CROSS_DELIVER).
- **Redemption vault:** batches RWA redemptions with the issuer, issues a tradeable claim token per
  batch, and runs in AUTO mode (verified issuer calls from per-batch redemption accounts) or MANUAL
  mode (a timelocked operator redeems off-chain against a NAV floor).
- **Governance:** every config, peer, route and vault change waits out a timelock a guardian can veto.
- **App and API:** in the Octarine UI a taker turns on "Allow cross-chain" and the auction opens to
  makers on the paired chains, on the same auction board. Each maker picks the chain it bids from (its
  USDC is pulled there; the backend picks that pair's bridge), names where the RWA lands, approves, and
  signs. A status panel shows each leg (fill, Circle attestation, payment,
  settlement) and gives each party its button: the maker relays the fill, anyone completes the
  payment for the tip, and the taker takes the RWA back if the trade fails. The Octarine backend
  builds and verifies requests, bids and every transaction, and tracks the trade through Circle's
  attestation API.

Done on testnet, against the real Circle CCTP V2 and LayerZero V2 contracts:

- a Stellar taker sold tTBILL to a Sepolia maker for USDC: the fill on Stellar, the relay on Sepolia
  (the maker's USDC burned back), and the `complete` on Stellar that paid the taker, the fee, the
  relayer's tip and delivered the RWA;
- forked-state tests against both chains' live bridge contracts, including USDT0 on an Ethereum
  mainnet fork.

## Deliverable Verification

- **Live transactions for every flow above**, run on 2026-10-30 on Stellar testnet and Sepolia, including
  each cross-chain delivery mode and the failure path: [`docs/DEMO_TRANSACTIONS.md`](DEMO_TRANSACTIONS.md).
- **Addresses and transaction hashes for every flow above:**
  [`docs/DEPLOYMENTS.md`](DEPLOYMENTS.md). The machine-readable run log is
  [`deployments/testnet-integration.json`](../deployments/testnet-integration.json).
- **Integration docs:** [`docs/integration`](integration/README.md).
- **Tests:** 196 contract tests (175 in the workspace plus 21 for the CCTP build of the
  cross-chain contract), at 90.9% line coverage (`make coverage`, excluding tests and mocks;
  contracts only loaded as wasm, such as the claim token, do not register). On top of those:
  - offline SDK and keeper tests;
  - the end-to-end testnet run `sdk/test/testnet.ts`, against Soroswap,
    Aquarius, Blend v2, DeFindex and the push oracle.
  - 18 forked-state tests (`e2e/`, `scripts/fork-test.sh`) running the cross-chain contracts against
    a snapshot of Stellar testnet's live CCTP and LayerZero contracts, and the EVM side's fork suites
    against Sepolia and Ethereum mainnet (`octarine-v2`, `test/fork`).

**Repositories:**

- contracts, SDK and keeper: [`stellar-rfq`](https://github.com/mystic-finance/Stellar-RFQ)
  (branch `milestone-2`);
- EVM side of cross-chain settlement: `octarine-v2` (branch `new-v2`, `src/octarine-rfq/crosschain`);
- frontend: [`Octarine-UI`](https://github.com/mystic-finance/Octarine-UI)
  (branch `stellar-facility-console`).
