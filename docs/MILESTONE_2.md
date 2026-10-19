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

## Deliverable Verification

- **Addresses and transaction hashes for every flow above:**
  [`docs/DEPLOYMENTS.md`](DEPLOYMENTS.md). The machine-readable run log is
  [`deployments/testnet-integration.json`](../deployments/testnet-integration.json).
- **Integration docs:** [`docs/integration`](integration/README.md).
- **Tests:** 138 contract tests, at 92.8% line coverage (`cargo llvm-cov`,
  excluding tests and mocks). On top of those:
  - offline SDK and keeper tests;
  - the end-to-end testnet run `sdk/test/testnet.ts`, against Soroswap,
    Aquarius, Blend v2, DeFindex and the push oracle.

**Repositories:**

- contracts, SDK and keeper: [`stellar-rfq`](https://github.com/mystic-finance/Stellar-RFQ)
  (branch `milestone-2`);
- frontend: [`Octarine-UI`](https://github.com/mystic-finance/Octarine-UI)
  (branch `facility-console`).
