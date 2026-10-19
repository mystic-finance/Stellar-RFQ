#!/usr/bin/env bash
# Build and optimise the contract WASMs.
source "$(dirname "$0")/lib.sh"
require_cmd stellar "Install: cargo install --locked stellar-cli"

log "Building contract WASM (wasm32v1-none, release)"
( cd "$ROOT_DIR" && stellar contract build )

for name in rfq router oracle push_oracle vault facility withdrawal_queue facility_aggregator dex_aggregator \
            soroswap_adapter aquarius_adapter sep56_adapter blend_adapter defindex_adapter \
            mock_venue mock_dex mock_oracle; do
  RAW="$WASM_RELEASE/$name.wasm"
  [ -f "$RAW" ] || die "expected $RAW not found"
  stellar contract optimize --wasm "$RAW"
  ok "$name -> $WASM_RELEASE/$name.optimized.wasm ($(du -h "$WASM_RELEASE/$name.optimized.wasm" | cut -f1))"
done
