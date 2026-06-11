#!/usr/bin/env bash
# Shared configuration and helpers for the Stellar RFQ scripts.
# Source this from every script: `source "$(dirname "$0")/lib.sh"`.
set -euo pipefail

# ---------------------------------------------------------------------------
# Paths
# ---------------------------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
DEPLOY_DIR="$ROOT_DIR/deployments"
WASM_RELEASE="$ROOT_DIR/target/wasm32v1-none/release"
mkdir -p "$DEPLOY_DIR"

# ---------------------------------------------------------------------------
# Network selection. Override with NETWORK=mainnet|testnet|futurenet|local.
# ---------------------------------------------------------------------------
NETWORK="${NETWORK:-testnet}"

case "$NETWORK" in
  testnet)
    RPC_URL="${RPC_URL:-https://soroban-testnet.stellar.org}"
    NETWORK_PASSPHRASE="Test SDF Network ; September 2015"
    FRIENDBOT=1 ;;
  futurenet)
    RPC_URL="${RPC_URL:-https://rpc-futurenet.stellar.org}"
    NETWORK_PASSPHRASE="Test SDF Future Network ; October 2022"
    FRIENDBOT=1 ;;
  mainnet|public)
    RPC_URL="${RPC_URL:-https://mainnet.sorobanrpc.com}"
    NETWORK_PASSPHRASE="Public Global Stellar Network ; September 2015"
    FRIENDBOT=0 ;;
  local|standalone)
    RPC_URL="${RPC_URL:-http://localhost:8000/rpc}"
    NETWORK_PASSPHRASE="Standalone Network ; February 2017"
    FRIENDBOT=1 ;;
  *)
    echo "Unknown NETWORK=$NETWORK" >&2; exit 1 ;;
esac

DEPLOYMENT_FILE="$DEPLOY_DIR/$NETWORK.json"
ACCOUNTS_FILE="$DEPLOY_DIR/accounts.$NETWORK.json"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
# Logging always goes to stderr so it never pollutes command substitutions that
# capture a script's stdout (e.g. contract ids).
log()  { printf '\033[1;34m==>\033[0m %s\n' "$*" >&2; }
ok()   { printf '\033[1;32m  ✓\033[0m %s\n' "$*" >&2; }
warn() { printf '\033[1;33m  !\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31mERROR:\033[0m %s\n' "$*" >&2; exit 1; }

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "'$1' is required but not installed. $2"
}

require_tools() {
  require_cmd stellar "Install: cargo install --locked stellar-cli"
  require_cmd jq "Install: brew install jq"
}

# Pass the network flags to every stellar CLI call.
stellar_net() {
  stellar "$@" --rpc-url "$RPC_URL" --network-passphrase "$NETWORK_PASSPHRASE"
}

# Read a field from the deployment JSON (e.g. `deployment .contracts.rfq`).
deployment() {
  [ -f "$DEPLOYMENT_FILE" ] || die "No deployment found at $DEPLOYMENT_FILE. Run 02-deploy.sh first."
  jq -r "$1" "$DEPLOYMENT_FILE"
}

# Read a field from the accounts JSON.
account() {
  [ -f "$ACCOUNTS_FILE" ] || die "No accounts file at $ACCOUNTS_FILE. Run 00-setup.sh first."
  jq -r "$1" "$ACCOUNTS_FILE"
}

# Merge a JSON fragment into a file (creates it if missing). Usage:
#   json_merge "$FILE" '.contracts.rfq = $v' --arg v "$CID"
json_merge() {
  local file="$1"; shift
  local filter="$1"; shift
  local tmp; tmp="$(mktemp)"
  if [ -f "$file" ]; then
    jq "$filter" "$@" "$file" > "$tmp"
  else
    echo '{}' | jq "$filter" "$@" > "$tmp"
  fi
  mv "$tmp" "$file"
}
