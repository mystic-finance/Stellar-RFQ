#!/usr/bin/env bash
# Build and optimise the contract WASMs.
source "$(dirname "$0")/lib.sh"
require_cmd stellar "Install: cargo install --locked stellar-cli"

log "Building contract WASM (wasm32v1-none, release)"
build_wasm

for name in rfq router feed_oracle push_oracle redemption_vault claim_token redemption_account call_verifier lz_settle cctp_settle verifier vault facility withdrawal_queue facility_aggregator dex_aggregator \
            soroswap_adapter aquarius_adapter sep56_adapter blend_adapter defindex_adapter \
            mock_venue mock_dex mock_oracle faucet; do
  RAW="$WASM_RELEASE/$name.wasm"
  [ -f "$RAW" ] || die "expected $RAW not found"
  stellar contract optimize --wasm "$RAW"
  ok "$name -> $WASM_RELEASE/$name.optimized.wasm ($(du -h "$WASM_RELEASE/$name.optimized.wasm" | cut -f1))"
done
