# RFQ and Dutch settlement on captured Blend testnet state

Builds the current rfq and feed_oracle contracts, then runs a combined settlement
scenario against the oracle configured by the captured testnet Blend pool.

## Run

Install Rust 1.95.0 with the wasm32v1-none target, then run from the repository root:

```sh
rustup toolchain install 1.95.0 --profile minimal --target wasm32v1-none
./integration-tests/blend/check.sh
# After dependencies have been downloaded:
./integration-tests/blend/check.sh --offline
```

The script builds the current checkout's contracts and replays committed captures.
No Stellar CLI, wallet, public-network deployment or RPC is needed for replay.
The isolated test workspace installs Kanatoko 30.0.0-rc.1 from crates.io; it does
not change the contracts' SDK dependencies. WASM outputs are ignored, rebuilt in
CI and never used as a substitute for the current source.

The root workspace has no committed lockfile. build.sh seeds a missing root
Cargo.lock from contract-deps.lock, then builds with --locked. An existing
root lockfile is preserved. When updating contract dependencies intentionally,
regenerate the root lockfile and copy it back to contract-deps.lock.

## Assertions

- Rolling RFQ: 500 XLM at 0.42 USDC, less 2.50 bps/day for 30 days, fills
  for 208.425 USDC with a 10-bps fee.
- A 100-XLM Dutch listing decays from 50 to 45 USDC after 3.5 days and fills.
- The remaining RFQ amount fills at the same price after that time advance;
  the captured Blend oracle is a fixed-price mock stamping the current time.
  Therefore this fixture cannot validate the stale-price rejection path.
- Exact taker/fee-recipient balances and no residual XLM at RFQ are checked.

## Captured state and boundaries

The unchanged Protocol 30 testnet capture is ledger 5648538, 2026-11-09
23:18:52 UTC. It contains the pool configuration, SEP-40 oracle and prices,
asset contracts and issuer account. Its oracle is read through the current
feed_oracle code. This complements existing controlled stale-feed unit tests;
it does not replace them with an always-fresh mock.

Funding and authorization are local/mocked. The SEP-53 signer is an explicit
ephemeral fixture key. Time advances 3.5 days while ledger sequence is fixed;
archival and network admission are not modeled. No on-chain transaction runs.

Explicit recapture uses RECAPTURE_RPC with cargo test after building. check.sh
clears that variable. Exact price assertions can fail if refreshed feeds change.
