#!/usr/bin/env bash
# Build and optimise the RFQ contract WASM.
source "$(dirname "$0")/lib.sh"
require_cmd stellar "Install: cargo install --locked stellar-cli"

log "Building contract WASM (wasm32v1-none, release)"
( cd "$ROOT_DIR" && stellar contract build )

RAW_WASM="$WASM_RELEASE/rfq.wasm"
[ -f "$RAW_WASM" ] || die "expected $RAW_WASM not found"
ok "built $RAW_WASM ($(du -h "$RAW_WASM" | cut -f1))"

log "Optimising WASM"
stellar contract optimize --wasm "$RAW_WASM"
OPT_WASM="$WASM_RELEASE/rfq.optimized.wasm"
[ -f "$OPT_WASM" ] && ok "optimised -> $OPT_WASM ($(du -h "$OPT_WASM" | cut -f1))"
