# SCF Build — Tranche Completion Form

## Are you ready to submit your next tranche of deliverables?

Yes.

## Project Stage

Pre-Launch #1 — MVP.

## Telegram Username

*(to fill in)*

## Tranche Deliverables

**1. RFQ Settlement Smart Contracts**: Holds the terms of a trade and executes
it. Both sides sign off-chain; the contract verifies the signatures and swaps the
assets in one step, taking the protocol fee in the same step. Supports RFQ orders
(the buyer bids a rate per day, the amount is derived at settlement), limit
orders, Dutch listings, partial fills, delegated signing keys, batch
cancellation, SEP-53 signatures and SAC allowances.

Done on testnet: 20+ Order fills.

**2. RFQ Router**: Settles the chosen trade in a single transaction, across one
or more liquidity sources. The seller sets a minimum output; if the trade would
deliver less, the whole transaction reverts and nothing moves. The router reads
and ranks prices from registered sources on-chain, but the route itself is chosen
off-chain by the seller and executed exactly as chosen — it never substitutes a
different one.

Done on testnet: 5+ routed settlements.

**3. Auction Backend, API & Frontend MVP**: The backend publishes the request,
collects and ranks bids, verifies every signature, and builds the transactions a
wallet signs. It holds no keys and submits nothing on a user's behalf. Market
makers can bid two ways: send a rate and get back the exact order to sign, or
build and sign the order themselves. The React frontend uses Stellar Wallets Kit,
so sellers create swaps and accept bids, and market makers bid, all signing from
their own wallet.

- Backend Docs at https://curator-api.mysticfinance.xyz/docs/#/Octarine
- UI Live at https://stellar-setup.octarine-ui.pages.dev

## Deliverable Verification — Video

## Additional Deliverable Verification

**Contracts (Stellar Testnet)**: open the Events tab on each for the fills above.

- Settlement: https://stellar.expert/explorer/testnet/contract/CDB75DJB7KK6V2CJPGT44CJRZYPP7BPXFHZTOPIYSGO2KGQC576UJYQM
- Router: https://stellar.expert/explorer/testnet/contract/CAVJVJ7QVIVJKR2DVBNBAPFGJLFL4PGW5QFFHVLWBGVWMDLDBPBT75P6
- Price adapter: https://stellar.expert/explorer/testnet/contract/CAGR33LOLRUMNYHGKE2P3Z55I67CMMCVTF2WNQITG526XAVPZGMMVNCC

**Test assets** — mUSDC `CBHHOLNFBQSZ7TJ4TE3UFF43HJ4XSBLU6AXDIC5K73Z4TZUQSUCNA7PC`,
mRWA `CCVU3HJIL4EZ2C3RT5ICQ7LAZ3IIF3TRKYBW72HD7JINH2IUD2PDRDD7`,
mXLM `CBOOCLAKDO4J3EDXLVZJ3EKSCIKGQ5NFYOEYNXPQTHSECTJWB26KA7M3`.

mXLM is priced from a live Reflector feed, so the system is demonstrably working
against a real market price.

**To test it**: open the app, connect a Stellar testnet wallet (xBull or any
Stellar Wallets Kit wallet), send us the address and we will mint you test
tokens, then create a swap and accept a bid. No login or credentials; everything
is signed in your own wallet.

**Repositories**:
contracts [`stellar-rfq`](https://github.com/mystic-finance/Stellar-RFQ), 
backend [`mystic-backend`](https://github.com/mystic-finance/backend/tree/alt-staging)
, and frontend [`Octarine-UI`](https://github.com/mystic-finance/Octarine-UI/tree/stellar-setup)

The backend and frontend repositories are private repositories, please share your github username so we'll give you access.

## Support Needed

**Price feeds for real-world assets.** Reflector covers crypto well and we use it
for mXLM, but most tokenised real-world assets are on no public feed, so
valuations have to come from the issuer. We would welcome a conversation with
anyone in the ecosystem working on this.
