#!/usr/bin/env bash
# Forked-state tests: cctp_settle against the real testnet CCTP V2 contracts and
# lz_settle against LayerZero V2's, from a ledger snapshot (see e2e/).
#
#   scripts/fork-test.sh            # build, then test against e2e/fixtures/testnet.json
#   scripts/fork-test.sh --refresh  # retake the snapshot from testnet first
#
# Refreshing needs a stellar CLI that reads the network's protocol (28+ for
# testnet's protocol 29); point STELLAR at it if the one on PATH is older.
# Tests that replay the Sepolia fork's bytes read
# ../octarine-v2/test/fork/fixtures/ and skip when it is absent.
source "$(dirname "$0")/lib.sh"

STELLAR="${STELLAR:-stellar}"
require_cmd "$STELLAR" "Install: https://github.com/stellar/stellar-cli/releases"
require_cmd cargo "Install: https://rustup.rs"

log "Building contract WASM (the tests load cctp_settle, lz_settle, rfq and verifier)"
build_wasm >/dev/null
ok "built"

SNAPSHOT="$ROOT_DIR/e2e/fixtures/testnet.json"
if [ "${1:-}" = "--refresh" ] || [ ! -f "$SNAPSHOT" ]; then
  require_cmd python3 "Install Python 3"
  major="$("$STELLAR" --version | head -1 | awk '{print $2}' | cut -d. -f1)"
  [ "$major" -ge 28 ] || die "stellar $major cannot read testnet's buckets; set STELLAR to a 28+ CLI"
  log "Snapshotting the testnet contracts (reads the history archive, a few minutes)"
  STELLAR="$STELLAR" python3 -I "$ROOT_DIR/e2e/snapshot.py" "$SNAPSHOT"
  ok "$SNAPSHOT"
fi

log "Running the forked-state tests"
( cd "$ROOT_DIR/e2e" && cargo test )
ok "fixtures for the EVM side: contracts/crosschain_settle/fixtures/"
