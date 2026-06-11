#!/usr/bin/env bash
# Deploy the (already-built) RFQ protocol contract and record its address.
# This is the forge-script equivalent: it does NOT compile and does NOT touch
# tokens — it just takes the prebuilt WASM, deploys it, initialises it, and
# writes deployments/<network>.json. Works against testnet OR mainnet.
#
#   NETWORK=testnet ./scripts/02-deploy.sh        # default
#   NETWORK=mainnet SOURCE=my-deployer ./scripts/02-deploy.sh
#
# Env:
#   SOURCE  stellar CLI identity (or secret key) to pay for + sign the deploy
#           (default: rfq-admin). Must be funded; no friendbot on mainnet.
#   ADMIN   admin address stored in the contract (default: SOURCE's address).
source "$(dirname "$0")/lib.sh"
require_tools

SOURCE="${SOURCE:-rfq-admin}"

# Resolve the deployer + admin addresses.
if stellar keys address "$SOURCE" >/dev/null 2>&1; then
  SOURCE_ADDR="$(stellar keys address "$SOURCE")"
else
  die "identity '$SOURCE' not found. Create it (./scripts/00-setup.sh) or pass SOURCE=<your-identity>."
fi
ADMIN_ADDR="${ADMIN:-$SOURCE_ADDR}"

WASM="$WASM_RELEASE/rfq.optimized.wasm"
[ -f "$WASM" ] || WASM="$WASM_RELEASE/rfq.wasm"
[ -f "$WASM" ] || die "No WASM found at $WASM_RELEASE. Build first: ./scripts/01-build.sh (or 'make wasm')."

NET=(--rpc-url "$RPC_URL" --network-passphrase "$NETWORK_PASSPHRASE")

if [ "$FRIENDBOT" = "0" ]; then
  warn "Deploying to '$NETWORK' with real funds (source: $SOURCE_ADDR). Ctrl-C to abort."
fi

log "Deploying RFQ contract to '$NETWORK' (wasm: $(basename "$WASM"))"
RFQ_ID="$(stellar contract deploy --wasm "$WASM" --source "$SOURCE" "${NET[@]}")"
ok "rfq -> $RFQ_ID"

log "Initialising (admin = $ADMIN_ADDR)"
stellar contract invoke --id "$RFQ_ID" --source "$SOURCE" "${NET[@]}" \
  -- initialize --admin "$ADMIN_ADDR" >/dev/null
ok "initialized"

TS="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
json_merge "$DEPLOYMENT_FILE" \
  '.network=$net | .rpcUrl=$rpc | .networkPassphrase=$pass | .deployedAt=$ts
   | .contracts.rfq=$rfq | .admin=$admin | .wasmFile=$wasm' \
  --arg net "$NETWORK" --arg rpc "$RPC_URL" --arg pass "$NETWORK_PASSPHRASE" \
  --arg ts "$TS" --arg rfq "$RFQ_ID" --arg admin "$ADMIN_ADDR" \
  --arg wasm "$(basename "$WASM")"

ok "wrote $DEPLOYMENT_FILE"
jq . "$DEPLOYMENT_FILE"
