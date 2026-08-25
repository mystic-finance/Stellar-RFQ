#!/usr/bin/env bash
# Deploy the (already-built) settlement contract and record its address.
# It does NOT compile and does NOT touch tokens — it takes the prebuilt WASM,
# deploys it, initialises it, and writes deployments/<network>.json.
#
#   NETWORK=testnet ./scripts/02-deploy.sh        # default
#   NETWORK=mainnet SOURCE=my-deployer REFERENCE=C... ./scripts/02-deploy.sh
#
# Env:
#   SOURCE     stellar CLI identity (or secret key) paying for + signing the deploy
#              (default: rfq-admin). Must be funded; no friendbot on mainnet.
#   ADMIN      admin address stored in the contracts (default: SOURCE's address).
#   REFERENCE  reference asset every pushed price is denominated in. Required on
#              mainnet; on test networks a demo token is deployed if unset.
source "$(dirname "$0")/lib.sh"
require_tools

SOURCE="${SOURCE:-rfq-admin}"

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

# The reference asset. On mainnet this is your settlement stablecoin.
REFERENCE_ADDR="${REFERENCE:-}"
if [ -z "$REFERENCE_ADDR" ]; then
  [ "$FRIENDBOT" = "0" ] && die "REFERENCE=<token contract id> is required on '$NETWORK'."
  TOKEN_WASM="$WASM_RELEASE/test_token.wasm"
  [ -f "$TOKEN_WASM" ] || die "No test_token WASM. Build first: ./scripts/01-build.sh"
  log "No REFERENCE given — deploying a demo reference token (OUSD)"
  REFERENCE_ADDR="$(stellar contract deploy --wasm "$TOKEN_WASM" --source "$SOURCE" "${NET[@]}")"
  stellar contract invoke --id "$REFERENCE_ADDR" --source "$SOURCE" "${NET[@]}" \
    -- initialize --admin "$ADMIN_ADDR" --decimal 7 --name OUSD --symbol OUSD >/dev/null
  ok "OUSD -> $REFERENCE_ADDR"
fi

log "Deploying settlement contract to '$NETWORK' (wasm: $(basename "$WASM"))"
RFQ_ID="$(stellar contract deploy --wasm "$WASM" --source "$SOURCE" "${NET[@]}")"
ok "rfq -> $RFQ_ID"

log "Initialising (admin = $ADMIN_ADDR, reference = $REFERENCE_ADDR)"
stellar contract invoke --id "$RFQ_ID" --source "$SOURCE" "${NET[@]}" \
  -- initialize --admin "$ADMIN_ADDR" --reference "$REFERENCE_ADDR" >/dev/null
ok "initialized"

TS="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
json_merge "$DEPLOYMENT_FILE" \
  '.network=$net | .rpcUrl=$rpc | .networkPassphrase=$pass | .deployedAt=$ts
   | .contracts.rfq=$rfq | .contracts.usd=$usd | .referenceAsset=$usd
   | .admin=$admin | .wasmFile=$wasm' \
  --arg net "$NETWORK" --arg rpc "$RPC_URL" --arg pass "$NETWORK_PASSPHRASE" \
  --arg ts "$TS" --arg rfq "$RFQ_ID" --arg usd "$REFERENCE_ADDR" \
  --arg admin "$ADMIN_ADDR" --arg wasm "$(basename "$WASM")"

# --- router ---------------------------------------------------------------
ROUTER_WASM="$WASM_RELEASE/router.optimized.wasm"
[ -f "$ROUTER_WASM" ] || ROUTER_WASM="$WASM_RELEASE/router.wasm"
[ -f "$ROUTER_WASM" ] || die "No router WASM at $WASM_RELEASE. Build first: ./scripts/01-build.sh"

log "Deploying RFQ router"
ROUTER_ID="$(stellar contract deploy --wasm "$ROUTER_WASM" --source "$SOURCE" "${NET[@]}")"
ok "router -> $ROUTER_ID"

stellar contract invoke --id "$ROUTER_ID" --source "$SOURCE" "${NET[@]}" \
  -- initialize --admin "$ADMIN_ADDR" --settlement "$RFQ_ID" >/dev/null
ok "router initialized against settlement $RFQ_ID"

json_merge "$DEPLOYMENT_FILE" '.contracts.router=$router' --arg router "$ROUTER_ID"

ok "wrote $DEPLOYMENT_FILE"
jq . "$DEPLOYMENT_FILE"
