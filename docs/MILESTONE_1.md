# Milestone 1 — Octarine on Stellar

**Status: complete and running on Stellar testnet.**

Octarine lets someone holding a real-world asset (a tokenised treasury bill, say)
sell it quickly, by having market makers compete to buy it. Milestone 1 built
that whole path on Stellar: the contracts that hold and move the money, the
service that runs the auction, and the interface people use.

---

## What we built

**Three smart contracts**, which are the parts that actually move funds:

| | What it does |
|---|---|
| **Settlement** | Holds the rules of a trade and executes it. Both sides approve off-chain; the contract checks the signatures and swaps the assets in one step. |
| **Router** | Takes the offer the seller picked and settles it in a single transaction. Built to handle several liquidity sources later; today it handles one. |
| **Price adapter** | Reads a live price feed and converts it into the format the settlement contract expects. |

## The idea that makes it different

Real-world assets can't be redeemed instantly — a treasury bill might take 7 or
30 days. A buyer bridging that wait deserves paying for it, and the longer the
wait, the more it costs.

So **buyers don't bid a price. They bid a rate, a small percentage per day.**
The actual amount is worked out at the moment the trade settles, using how many
days are left and the live market price.

The practical benefit: a buyer signs their offer **once** and it stays correct as
time passes. With a fixed price, every tick of the clock would make the offer
stale and it would have to be re-signed.

The seller is protected two ways: they set the most they'll pay per day, and a
floor on what they must receive. Both are locked in before any buyer bids, so an
offer can't quietly move the terms.

## Proven working, end to end

Live on testnet with real signatures and real tokens moving not simulated at **https://stellar-setup.octarine-ui.pages.dev**:

- A seller creates a request and signs their terms
- A buyer bids a rate and signs the offer
- The trade settles through the router in one transaction
- Fees are taken, and cancelled offers are correctly refused

Backed by **55 contract tests** and **344 service tests**, all passing.

## Connected up

- **Backend** — runs the auction, verifies signatures, prepares the transactions
  for people's wallets. Buyers can either let us price their rate and hand them
  something to sign, or build and sign it themselves — whichever suits them.
- **Interface** — sellers create requests, buyers bid, sellers accept, all with a
  Stellar wallet (Freighter and others).
- **Test assets** — three tokens on testnet (`mUSDC`, `mRWA`, `mXLM`), funded to
  every test wallet. One is on a live market price feed; the others use a price
  we set.

---

## Where it stands

Everything above is **on testnet**, which is the point of this milestone: prove
the mechanism works before real money touches it.

## Reference

Stellar testnet. Current values live in `deployments/testnet.json`.

| | |
|---|---|
| Settlement | `CDB75DJB7KK6V2CJPGT44CJRZYPP7BPXFHZTOPIYSGO2KGQC576UJYQM` |
| Router | `CAVJVJ7QVIVJKR2DVBNBAPFGJLFL4PGW5QFFHVLWBGVWMDLDBPBT75P6` |
| Price adapter | `CAGR33LOLRUMNYHGKE2P3Z55I67CMMCVTF2WNQITG526XAVPZGMMVNCC` |

More detail: [`TECHNICAL_ARCHITECTURE.md`](./TECHNICAL_ARCHITECTURE.md) and the
[project README](../README.md).
