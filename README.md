# Stellar RFQ — on-chain RFQ & limit-order settlement for Soroban

A Soroban (Rust) settlement contract for **RFQ** and **limit** orders. Makers
sign orders off-chain (ed25519); takers submit them on-chain and the contract
settles the trade atomically, moving the maker and taker tokens through the
Soroban token interface. It supports partial fills, fill-or-kill, single and
pair cancellation, a maker-side fee, and signer/origin registries.

This is a **standalone contracts + deployment-scripts** repository. Any
client/SDK can build, sign and submit orders against the deployed contract using
the addresses written to [`deployments/`](deployments/).

---

## What it does

A maker authorises a price off-chain, a taker fills it on-chain, and settlement
is a pair of token transfers with proportional partial-fill accounting,
cancellation, and fee handling.

### RFQ order vs. limit order

Both are maker-signed and settle through the same core; they differ in gating
and fees:

- **RFQ order** — request-for-quote. Gated by `tx_origin` so only the intended
  submitter/relayer (or whitelisted origins) can land the fill, which gives
  strong MEV protection. No fee, cheapest to settle, short-lived. **The
  workhorse for quote-driven swaps.**
- **Limit order** — a resting order fillable by anyone (or a designated taker),
  with an optional fee skimmed from the maker output to a fee recipient. Use it
  for standing, openly-fillable liquidity (an order book).

---

## Authorisation model

Because the maker is offline at fill time, authorisation is split in two:

- **Custody** comes from a pre-existing token **allowance**: the maker and taker
  each `approve` this contract as a spender. Settlement uses `transfer_from`
  with this contract as the spender.
- **Per-order intent** comes from the maker's **ed25519 signature** over the
  canonical order hash. A signature only counts if its public key is registered
  to the maker via `register_order_signer`, which binds the signing key to the
  maker's address and prevents key-spoofing.
- **Submission gating** maps to checking the on-chain submitter (the `taker`)
  against the order's `tx_origin` and the allowed-origin registry.

The order hash is `sha256(DOMAIN_TAG || contract_address_xdr || order_xdr)`. The
contract address in the preimage binds a signature to a specific deployment. An
off-chain signer never re-implements the byte layout — it calls the read-only
`get_rfq_order_hash` / `get_limit_order_hash` to get the exact 32-byte digest,
then ed25519-signs it.

---

## Layout

```
contracts/
  rfq/          The settlement contract (RFQ + limit orders)
    src/
      lib.rs        contract entry points + settlement core
      types.rs      order / signature / status structs
      hash.rs       canonical, domain-separated order hashing
      storage.rs    persistent storage layout & accessors
      errors.rs     contract error codes
      test.rs       unit tests (signing, partial fills, cancel, fees, …)
  test_token/   Minimal Soroban token for tests/demos (no trustlines)
scripts/
  lib.sh           shared config (networks, helpers)
  00-setup.sh      generate + fund identities (admin/maker/taker)
  01-build.sh      build + optimise the WASM
  02-deploy.sh     deploy the contract + initialize  (testnet OR mainnet)
  03-seed-demo.sh  testnet-only: deploy test tokens, mint, register signer
  e2e.sh           run the testnet pipeline end to end
  strkey.mjs       decode a G... address to a raw ed25519 pubkey (deploy helper)
deployments/
  <network>.json            deployed addresses (the hand-off artifact)
  accounts.<network>.json   identity addresses + secrets (git-ignored)
```

---

## Prerequisites

- Rust + the `wasm32v1-none` target: `rustup target add wasm32v1-none`
- [`stellar-cli`](https://developers.stellar.org/docs/tools/cli): `cargo install --locked stellar-cli`
- `jq` and `node` (used by the deploy scripts)

---

## Build & test

```bash
make build      # cargo build (host)
make test       # run the unit-test suite
make wasm       # build + optimise the on-chain WASM
```

The contract WASM is ~20 KB optimised.

## Deploy

Build once, then deploy the prebuilt WASM.

```bash
# Testnet (default): create + fund identities, then deploy.
make setup
make wasm
make deploy                       # -> deployments/testnet.json

# Mainnet: use your own funded deployer identity.
NETWORK=mainnet SOURCE=my-deployer make deploy
```

`02-deploy.sh` deploys the contract, calls `initialize(admin)`, and writes the
addresses to `deployments/<network>.json`:

```json
{
  "network": "testnet",
  "rpcUrl": "https://soroban-testnet.stellar.org",
  "networkPassphrase": "Test SDF Network ; September 2015",
  "deployedAt": "2026-06-11T08:47:23Z",
  "contracts": { "rfq": "CBI6…PJHH" },
  "admin": "GBH2…3PN2",
  "wasmFile": "rfq.optimized.wasm"
}
```

### Testnet demo fixtures (optional)

```bash
make seed-demo    # deploys RFQA/RFQB test tokens, mints to maker/taker,
                  # registers the maker's signing key, updates the JSON
```

Or run the whole testnet pipeline at once: `make e2e`.

### Mint test tokens

The test tokens are admin-gated; the `rfq-admin` identity mints to any account:

```bash
make mint TO=G...address                      # mints 1000 RFQA + 1000 RFQB (default)
make mint TO=G...address AMOUNT=50000000000   # custom raw amount (7 decimals)
```

`SOURCE`, `ADMIN`, `NETWORK`, `RPC_URL`, and `MINT_AMOUNT` can be overridden via
environment variables (see the script headers).

---

## Contract API (entry points)

```
initialize(admin)
get_admin()
upgrade(new_wasm_hash)                               # admin auth (Soroban upgrade)

register_order_signer(maker, signer_pubkey, allowed) # maker auth
register_allowed_rfq_origin(origin_owner, submitter, allowed)
is_order_signer(maker, signer_pubkey)

get_rfq_order_hash(order)   / get_limit_order_hash(order)     # sign these
get_rfq_order_info(order)   / get_limit_order_info(order)     # status + filled

fill_rfq_order(order, signature, taker, fill_amount)
fill_or_kill_rfq_order(order, signature, taker, fill_amount)
fill_limit_order(order, signature, taker, fill_amount)
fill_or_kill_limit_order(order, signature, taker, fill_amount)

cancel_rfq_order(order) / cancel_limit_order(order)               # maker auth
cancel_pair_rfq_orders(maker, maker_token, taker_token, min_salt) # maker auth
cancel_pair_limit_orders(maker, maker_token, taker_token, min_salt)
```

### Settlement core

```
taker_filled = min(fill_amount, taker_amount − already_filled)
maker_filled = floor(taker_filled × maker_amount / taker_amount)
```

The filled amount is recorded **before** any transfer. For limit orders the fee
is skimmed from the **maker output**:

```
fee_filled = floor(maker_filled × token_fee_amount / maker_amount)   # to fee_recipient
taker receives maker_filled − fee_filled
```

All proportional math uses 256-bit intermediates (`mul_div_floor`) to avoid
`i128` overflow.

### Integration recipe (client/SDK)

1. Maker `approve`s the RFQ contract for the maker token; taker for the taker
   token.
2. Maker calls `register_order_signer(maker, pubkey, true)` once.
3. Build an order, read its hash via `get_rfq_order_hash`, ed25519-sign the hash
   with the maker's key → `{ signer: pubkey, signature }`.
4. Taker submits `fill_rfq_order(order, signature, taker, amount)`.

---

## Design notes

- **Signatures are ed25519.** Stellar accounts are ed25519, so makers sign the
  32-byte digest from `get_*_order_hash` (a domain-separated sha256 over the
  order XDR). The signing key must be registered to the maker via
  `register_order_signer` — including the maker's own primary key, since a
  Stellar address is not its raw public key on-chain.
- **`tx_origin` is the on-chain submitter.** Soroban has no `tx.origin`, so the
  RFQ origin gate is checked against the `taker` that authorises the fill (plus
  the allowed-origin registry). For the usual taker-submits flow this is exactly
  what you want.
- **Amounts are `i128`** — the native Soroban token-interface amount type. `salt`
  is `u64` (Soroban `contracttype` has no native 256-bit integer); the order
  builder generates a 64-bit salt.
- **Allowances expire.** `approve` on Soroban tokens carries an
  `expiration_ledger`; makers/takers refresh approvals as needed.
- **Upgrades** use Soroban's native `upgrade(new_wasm_hash)`, gated by the admin.

## Security notes

- Settlement records the fill **before** moving funds and re-checks order status
  (`Fillable`) on every call; expired/cancelled/over-filled orders revert.
- The signer registry binds an ed25519 key to a maker address, so a third party
  cannot craft an order against a victim's allowance with their own key.
- The order hash is domain-separated by contract address, so signatures are not
  replayable across deployments.
- `test_token` is a throwaway demo token — never deploy it to mainnet.

## License

Apache-2.0
