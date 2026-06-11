# Octarine 
## Technical Infrastructure Description

---

# Introduction

## High-Level Overview

Octarine is a protocol enabling instant liquidity for RWAs by auctioning liquidations with institutional LPs, third-party protocols and curated liquidity facilities.

RWAs struggle to get DEX liquidity because they're fragmented, regulated and tend to have built-in redemption times. These are not instant, however, which means that RWAs can't be onboarded in lending, as DeFi liquidations need instant liquidity. It also doesn't allow managing leveraged loops, as users can't instantly unwind their positions. To top it all off, the absence of instant liquidity makes RWAs less attractive for prospective LPs, who can't afford to have their capital locked up unwillingly. This ultimately translates to less TVL for issuers and a total lack of RWA DeFi utility, as these assets can't be trade nor onboarded as collateral.

Octarine changes that, by giving liquidity to RWAs via auctions with LPs, third-party protocols and curated liquidity facilities. When there is a liquidation, either on Octarine or on a connected venue, Octarine detects it and auctions it with connected bidders. We then award the trade to the best bid available. Bids can come from either LPs bidding on the API or from DeFi protocols quoting prices for buying RWAs, thus ensuring the RFQ always has the best price in the market.

In order to service liquidations at DeFi rates, Octarine features also a primitive enabling the curation of instant liquidity facilities, which are vaults that store deposits in liquid strategies like lending. These vaults service liquidations by calling capital from lending strategies and sending it to the user, whilst then redeeming the RWA and taking a haircut in the process. This approach makes the trade modular in DeFi and enables deep, ecosystem-wide integrations.

Serves this document to outline the beginning of how Octarine is bringing this to Stellar. In particular, this codebase features a settlement mechanism enabling Octarine auctions to take place and be completed on Stellar. How it works is simple: a user submits a swap request, the platform runs a short auction amongst connected bidders and then gives the trade to the winning bid. Trade is then settled in a single atomic transaction, exchanging each party¡s assets and routing the protocol fee.

A super simple version of this is **already working on Stellar Testnet today**, and deployed with the contract `CAPVBMQBVQVDFDWFGH4M3EJH7CYM7MWIYE5TOYTYASOU26L2Q4T2YJZW`. This document describes the production architecture the protocol aspires to ultimately build out — smart contracts, backend, and frontend.

## Definitions, acronyms and abbreviations

- **RFQ** – Request‑for‑Quote: an auction sale of an asset by a maker, who creates it via a signed quote that a taker fills on‑chain.
- **Maker** – The party bidding and signing the order (gives the maker token).
- **Taker** – The party filling the order (gives the taker token).
- **Fill / Partial fill** – Executing an order fully or for a portion of its amount.
- **Soroban** – Stellar's smart contract platform.
- **SEP‑41** – Stellar's standard token interface (`approve`, `transfer_from`, `balance`).
- **SEP‑53** – Stellar's message‑signing standard; the analogue of EVM's EIP‑712.
- **strkey** – Stellar's address encoding (`G…` accounts, `C…` contracts).
- **Allowance** – Permission given to someone to spend a certain amount of a token (carries an `expiration_ledger`).
- **Fee recipient** – The address that receives the protocol fee (taken from the maker's output).
- **XDR** – Stellar's canonical binary serialization (used for order hashing).
- **Soroban RPC** – The JSON‑RPC endpoint for simulating and submitting Soroban transactions.

## Architecture constraints

- **Non‑custodial & keyless backend** – every value‑moving action is signed by a
  user/maker wallet; the backend holds no keys and no funds.
- **Atomic settlement** – both token legs and the fee move in one transaction or
  the whole fill reverts (no partial settlement state).
- **Wallet‑ and bot‑producible signatures** – maker orders must be signable by
  browser wallets (xBull/Freighter) *or* programmatic makers, with one
  scheme (SEP‑53).
- **Replay safety** – signatures must be bound to a specific deployment (domain
  separation) and network (SEP‑53 passphrase).
- **Token model** – assets follow **SEP‑41**; allowances are explicit and expire.
- **Networks** – Stellar **testnet** and **mainnet** only; deterministic,
  reproducible deploys via stellar‑cli.

---

# Architecture Overview

## C4 L1 Diagram: High-Level Architecture (System Context)

```


          Taker                                            Maker
   swaps assets via a wallet                   quotes & signs orders (bot / desk)

            │  uses                                          │  posts signed bids
            └────────────────────┐          ┌────────────────┘
                                  ▼          ▼

          ╔═══════════════════════════════════════════════════════╗
          ║                    OCTARINE  PLATFORM                  ║
          ║                                                        ║
          ║      RFQ swap & auction marketplace — runs the         ║
          ║      bid auction and settles trades on Soroban.        ║
          ╚════════════╤═══════════════╤════════════════╤══════════╝
                       │               │                │
           signs tx &  │     token     │    identity    │
           messages    │   transfers   │   (optional)   │
                       ▼               ▼                ▼

       Stellar Wallets Kit      SEP-41 Token        KYC / Compliance
       (xBull, Freighter)       Contracts           Provider
                                (maker / taker)      (roadmap)
```

## C4 L2 Diagram: Zoom into the Octarine System (Containers)

```
       Taker                                                 Maker
         │  swap                                               │  POST /bid (signed)
         ▼                                                     ▼

   ╔════════════════════════════════════════════════════════════════════════════╗
   ║   OCTARINE  PLATFORM                                                       ║
   ║                                                                            ║
   ║   ┌──────────────────────────┐         ┌────────────────────────────────┐  ║
   ║   │  Frontend                │ REST API│  Backend                       │  ║
   ║   │  React / Vite            │────────▶│  NestJS (TypeScript)           │  ║
   ║   │                          │         │                                │  ║
   ║   │  • Stellar Wallets Kit   │◀────────│  • swap / bid / fill API       │  ║
   ║   │  • swap · auctions ·     │ signed  │  • bid auction & matching      │  ║
   ║   │    bid · dashboard       │ ops     │  • SEP-53 signature verify     │  ║
   ║   └────────────┬─────────────┘         │  • Soroban op assembly         │  ║
   ║                │                        │  • dispatch by chainId        │  ║
   ║                │                        └───────────────┬───────────────┘  ║
   ║                │                                         │  read / write   ║
   ║                │                                         ▼                 ║
   ║                │                        ┌────────────────────────────────┐ ║
   ║                │                        │  MongoDB                       │ ║
   ║                │                        │  requests · bids ·             │ ║
   ║                │                        │  approvals · token registry    │ ║
   ║                │                        └────────────────────────────────┘ ║
   ╚════════════════╪═══════════════════════════════════════════════════════════╝
                    │
                    │   invoke (wallet-signed)   +   simulate (read-only)
                    ▼

   ╔═══════════════════════════════════════════════════════════════════════════╗
   ║   STELLAR / SOROBAN                                                       ║
   ║                                                                           ║
   ║   ┌───────────────────────────────────┐ transfer_from ┌─────────────────┐ ║
   ║   │  Settlement Contract (Rust / WASM) │──────────────▶│  SEP-41 Tokens │ ║
   ║   │  verify · fill · fee · cancel      │               │ (maker / taker)│ ║
   ║   └───────────────────────────────────┘               └─────────────────┘ ║
   ╚═══════════════════════════════════════════════════════════════════════════╝
```

**Trust boundary.** The **only trusted component is the Soroban settlement
contract**. The backend coordinates the order book and *assembles* operations but
is **keyless and fundless**; the user's wallet signs everything that moves value.

## Contract Overview

**Smart Contract Objective:** A Soroban contract that settles **maker‑signed RFQ
and limit orders** between two SEP‑41 tokens. It verifies the maker's signature,
computes the proportional fill, atomically swaps the two legs via
`transfer_from`, and skims the protocol fee from the maker's output.

**Key Functions:**

- **Fill (`fill_limit_order` / `fill_rfq_order`)** → Taker submits a maker‑signed
  order; the contract verifies the SEP‑53 signature, clamps the fill to the
  remaining amount, and settles. `fill_or_kill_*` variants require an exact fill or revert.
- **Settlement math** → `taker_filled = min(fill, taker_amount − filled)`;
  `maker_filled = floor(taker_filled × maker_amount / taker_amount)`;
  `fee = floor(maker_filled × token_fee_amount / maker_amount)` (256‑bit
  intermediates avoid `i128` overflow). Filled state is persisted **before** any
  transfer.
- **Signature verification (SEP‑53)** → The maker signs the order hash as a SEP‑53
  message; the contract recomputes `SHA256("Stellar Signed Message:\n" ‖
  order_hash)` and `ed25519_verify`s it. A maker signing its **own** order needs
  no registration (its ed25519 key is recovered from its `G…` address); delegated
  hot keys are authorized via `register_order_signer`.
- **Order hashing** → `sha256(DOMAIN ‖ contract_address_xdr ‖ order_xdr)` —
  domain‑separated by deployment, reproducible off‑chain via a read‑only
  `get_*_order_hash` simulation.
- **Cancellation** → `cancel_{rfq,limit}_order` (single) and `cancel_pair_*`
  (invalidate all of a maker's orders for a pair below a salt).
- **Fees & MEV** → Limit orders carry `token_fee_amount` → `fee_recipient`; RFQ
  orders are `tx_origin`‑gated for MEV protection.
- **Admin** → `initialize(admin)` and native `upgrade(wasm_hash)`.


**Settlement flow (request → competitive bid → fill):**

```
 Taker(UI)        Backend          Maker/MM           Soroban
   │ POST /swap     │                   │                 │
   ├───────────────▶│ create request    │                 │
   │ poll /swap/:id │   POST /bid  ◀────┤ build order     │
   │                │                   │                 │
   │                │  verify SEP-53 ──▶│ signMessage &   │
   │                │  (signer==maker)  │ approve token   │ approve()
   │                │                   │                 │
   │ bid + ops  ◀───┤  assemble ops     │                 │
   │ sign+submit ───┼───────────────────────────────────▶ fill_limit_order
   │  (wallet)      │                  │                 │  verify · transfer×2 · fee
   ▼                ▼                  ▼                 ▼
        taker token: taker→maker        maker token: maker→taker (− fee)
```

---

# Technology Stack

## Backend

- **NestJS (TypeScript)** – Main API backend; the Stellar engine is a module that
  plugs into the existing multi‑chain order/bid/fill surface and writes to **
  MongoDB documents**, saving the auction, bids and settlement data.
- **Order / bid / fill lifecycle** – `POST /swap` (create request), `POST /bid`
  (verify maker SEP‑53 signature, store competing quote), `GET /swap/:id` (best
  bids + ready‑to‑sign ops), `POST /fill` & `/approval` (record settlement).
- **Soroban op assembly (keyless)** – Returns base64 `InvokeHostFunction`
  operations (`approve` + `fill_limit_order`), the Soroban analogue of EVM
  calldata, for the wallet to execute. Never signs or holds funds.
- **Soroban reads** – Simulates `get_*_order_hash`, `get_*_order_info`,
  `is_order_signer`, and token `balance` for signature verification, status, and
  pricing.
- **MongoDB** – Requests, bids, approvals, and the token registry.

## Frontend

- **React + Vite (TypeScript)** – Swap page, auctions/bid board, market‑maker bid
  flow, dashboard, and live balances.
- **Stellar Wallets Kit** – Single integration for **xBull & Freighter** (plus
  Albedo, Rabet, Lobstr, Hana): `signTransaction` for fills, `signMessage`
  (SEP‑53) for orders.
- **Soroban interaction** – Builds/submits transactions from the backend's ops,
  simulates balances, and reads token metadata.

## Infrastructure

- **Azure Virtual Machines** – Host the NestJS backend and the
  frontend build, with **nginx** as the reverse proxy / TLS terminator in front of
  the API and static assets.
- **MongoDB on Azure** – Order book, bids, approvals, and the token registry,
  hosted on Azure for low‑latency access from the API VMs.
- **Cloudflare Pages** – Static hosting + CDN for the frontend deployment.
- **Soroban RPC** – `soroban-testnet.stellar.org` (testnet) / mainnet RPC for
  simulate + submit; Horizon for account data.
- **stellar-cli pipeline** – Deterministic build → optimize (~21 KB WASM) →
  deploy → `initialize`, with addresses written to `deployments/<network>.json`.

## Integrations

- **Stellar Wallets Kit** (xBull, Freighter, …) – Wallet connection + tx/message
  signing.
- **Soroban RPC / Horizon** – Simulation, submission, balance and ledger queries.
- **SEP‑41 token contracts** – The maker/taker assets settled by the engine.
- **Stellar Lab** – PoC inspection of XDR, contract invocations, and signed
  transactions during development.
- **KYC / Compliance provider** *(roadmap)* – Gating for regulated assets, mirrored
  from the existing EVM compliance hooks.

---

# Status & Roadmap

**Built and verified on testnet:** RFQ + limit settlement contracts (SEP‑53
signing, partial fills, fee‑from‑maker‑output, cancellation, fill‑or‑kill,
upgradeability); NestJS order/bid/fill API as a peer to the EVM chains; React UI
with xBull/Freighter and a live `swap → competitive bid → on‑chain fill`.

**Next:** mainnet deployment + security audit · multi‑maker aggregation &
best‑price routing · standing/recurring orders · protocol‑fee treasury ·
contract‑events indexer for a trade feed · public SDK + reference market‑maker bot.

*Repository: `contracts/rfq` (settlement), `contracts/test_token` (SEP‑41 demo
token), `scripts/` (build/deploy/seed via stellar‑cli),
`deployments/<network>.json` (addresses).*
