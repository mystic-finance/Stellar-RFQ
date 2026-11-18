# RFQ and Dutch fills on Blend testnet prices

This example uses [Kanatoko](https://crates.io/crates/kanatoko) to run the current RFQ and feed adapter with the SEP-40 oracle configured by a captured Blend testnet pool. It follows one taker and fee recipient through three fills.

| Fill | Observed result |
| --- | --- |
| First 500 XLM of a rolling RFQ | 208.425 USDC before the 10 bps fee |
| 100 XLM Dutch listing after 3½ days | Ask falls from 50 to 45 USDC. The fee is 0.045 USDC. |
| Remaining 500 XLM of the RFQ | Same price and fee as the first fill. The final assertions check both recipients’ balances. |

The testnet oracle returns a fixed price and stamps each read with the current time. Advancing the clock therefore cannot exercise stale-price rejection. The test uses a local signing key and the frozen Protocol 30 [testnet fixture](fixtures/testnet-blend.json) from ledger 5648538.

[Read the Rust assertions](tests/testnet_blend.rs#L120).

## Reproduce locally

Use Rust 1.95.0 with the `wasm32v1-none` target. From the repo root, build the contracts, fetch the locked test dependencies once, then run the test offline. The first command supplies the recorded contract lockfile on checkouts that do not already have one.

```sh
cp -n integration-tests/blend/contract-deps.lock Cargo.lock
cargo +1.95.0 build --locked --manifest-path Cargo.toml --target wasm32v1-none --profile release -p rfq -p feed_oracle --target-dir integration-tests/blend/target
cargo +1.95.0 fetch --locked --manifest-path integration-tests/blend/Cargo.toml
cargo +1.95.0 test --locked --offline --manifest-path integration-tests/blend/Cargo.toml
```
