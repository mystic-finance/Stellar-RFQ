#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
workspace="$root/."
target_dir="${CARGO_TARGET_DIR:-$workspace/target}"
mkdir -p "$here/wasm"
if [[ ! -f "$workspace/Cargo.lock" ]]; then
  cp "$here/contract-deps.lock" "$workspace/Cargo.lock"
fi
cd "$workspace"
cargo +1.95.0 build --locked --release --target wasm32v1-none --package rfq --package feed_oracle
cp "$target_dir/wasm32v1-none/release/rfq.wasm" "$here/wasm/rfq.wasm"
cp "$target_dir/wasm32v1-none/release/feed_oracle.wasm" "$here/wasm/feed_oracle.wasm"
