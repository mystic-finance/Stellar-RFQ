#!/usr/bin/env bash
# Regenerates src/contracts/<name>.ts from the built WASMs (run ../scripts/01-build.sh first).
set -euo pipefail
cd "$(dirname "$0")/.."
WASM=../target/wasm32v1-none/release
TMP="$(mktemp -d)"
for name in rfq router push_oracle vault facility withdrawal_queue facility_aggregator dex_aggregator; do
  stellar contract bindings typescript --wasm "$WASM/$name.wasm" --output-dir "$TMP/$name" --overwrite >/dev/null
  # Keep the client and types; drop the package's re-exports of the whole SDK.
  sed -e '/^export \* from "@stellar\/stellar-sdk";$/d' \
      -e '/^export \* as contract from/d' -e '/^export \* as rpc from/d' \
      -e '/^if (typeof window !== "undefined") {$/,/^}$/d' \
      -e 's/Array<InvokerContractAuthEntry>/Array<unknown>/' \
      "$TMP/$name/src/index.ts" > "src/contracts/$name.ts"
  echo "src/contracts/$name.ts" >&2
done
# Shared interfaces: any SEP-41 token (the demo token speaks it) and any venue
# adapter (Blend, DeFindex and SEP-56 adapters expose the same VenueAdapter).
for pair in token:test_token venue:sep56_adapter; do
  out="${pair%%:*}"; wasm="${pair##*:}"
  stellar contract bindings typescript --wasm "$WASM/$wasm.wasm" --output-dir "$TMP/$out" --overwrite >/dev/null
  sed -e '/^export \* from "@stellar\/stellar-sdk";$/d' -e '/^export \* as contract from/d' -e '/^export \* as rpc from/d' \
      -e '/^if (typeof window !== "undefined") {$/,/^}$/d' "$TMP/$out/src/index.ts" > "src/contracts/$out.ts"
done
rm -rf "$TMP"
