#!/usr/bin/env bash
# Milestone 2 protocol: the facility aggregator (vault factory + router source),
# the DEX aggregator, and the venue / DEX adapter code. Run after 02-deploy.sh;
# reads the router from deployments/<network>.json. Facilities price RWA from
# their own oracles, so nothing here depends on the settlement contract.
#
#   NETWORK=testnet ./scripts/08-deploy-facility.sh
#
# Env:
#   SOURCE            identity paying for and signing (default: rfq-admin)
#   FEE_RECIPIENT     protocol fee recipient (default: the admin)
#   FACILITY_FEE_BPS  facility aggregator protocol fee, <= 500 (default: 50)
#   DEX_FEE_BPS       DEX aggregator protocol fee, <= 100 (default: 10)
#   SOROSWAP_ROUTER   Soroswap router contract; deploys the Soroswap adapter if set
#   AQUARIUS          set to 1 to deploy the Aquarius adapter (pools are named per hop)
#   KEEPER            push-oracle updater (default: the admin)
#   MAX_DEVIATION_BPS largest move an updater may push (default: 200)
source "$(dirname "$0")/lib.sh"
require_tools

SOURCE="${SOURCE:-rfq-admin}"
ADMIN_ADDR="$(stellar keys address "$SOURCE")" || die "identity '$SOURCE' not found"
FEE_RECIPIENT="${FEE_RECIPIENT:-$ADMIN_ADDR}"
ROUTER_ID="$(deployment .contracts.router)"
NET=(--source "$SOURCE" --rpc-url "$RPC_URL" --network-passphrase "$NETWORK_PASSPHRASE")

wasm() {
  local w="$WASM_RELEASE/$1.optimized.wasm"
  [ -f "$w" ] || w="$WASM_RELEASE/$1.wasm"
  [ -f "$w" ] || die "No $1 WASM. Build first: ./scripts/01-build.sh"
  echo "$w"
}
upload() { stellar contract upload --wasm "$(wasm "$1")" "${NET[@]}"; }
invoke() { local id="$1"; shift; stellar contract invoke --id "$id" "${NET[@]}" -- "$@" >/dev/null; }

log "Uploading vault, facility and queue code"
VAULT_HASH="$(upload vault)"
FACILITY_HASH="$(upload facility)"
QUEUE_HASH="$(upload withdrawal_queue)"
ok "vault $VAULT_HASH, facility $FACILITY_HASH, queue $QUEUE_HASH"

log "Deploying facility aggregator"
FAC_ID="$(stellar contract deploy --wasm "$(wasm facility_aggregator)" "${NET[@]}" -- \
  --admin "$ADMIN_ADDR" --router "$ROUTER_ID" \
  --wasms "{\"vault\":\"$VAULT_HASH\",\"facility\":\"$FACILITY_HASH\",\"queue\":\"$QUEUE_HASH\"}" --fee_recipient "$FEE_RECIPIENT" --fee_bps "${FACILITY_FEE_BPS:-50}")"
ok "facility_aggregator -> $FAC_ID"

log "Deploying DEX aggregator"
DEX_ID="$(stellar contract deploy --wasm "$(wasm dex_aggregator)" "${NET[@]}" -- \
  --admin "$ADMIN_ADDR" --router "$ROUTER_ID" \
  --fees "{\"recipient\":\"$FEE_RECIPIENT\",\"protocol_fee_bps\":${DEX_FEE_BPS:-10},\"max_partner_fee_bps\":300,\"partner_share_bps\":2000}")"
ok "dex_aggregator -> $DEX_ID"

log "Registering both as router sources"
invoke "$ROUTER_ID" register_source --source "$FAC_ID" --kind Facility --allowed true
invoke "$ROUTER_ID" register_source --source "$DEX_ID" --kind Dex --allowed true
ok "router routes Facility -> $FAC_ID, Dex -> $DEX_ID"

log "Allowlisting venue adapter code"
SEP56_HASH="$(upload sep56_adapter)"
BLEND_HASH="$(upload blend_adapter)"
DEFINDEX_HASH="$(upload defindex_adapter)"
for h in "$SEP56_HASH" "$BLEND_HASH" "$DEFINDEX_HASH"; do
  invoke "$FAC_ID" allow_adapter --wasm_hash "$h" --allowed true
done
ok "sep56_adapter $SEP56_HASH, blend_adapter $BLEND_HASH, defindex_adapter $DEFINDEX_HASH"
MOCK_VENUE_HASH=""
if [ "$FRIENDBOT" = "1" ]; then
  MOCK_VENUE_HASH="$(upload mock_venue)"
  invoke "$FAC_ID" allow_adapter --wasm_hash "$MOCK_VENUE_HASH" --allowed true
  ok "mock_venue $MOCK_VENUE_HASH (test networks only)"
fi

SOROSWAP_ID=""
if [ -n "${SOROSWAP_ROUTER:-}" ]; then
  log "Deploying the Soroswap adapter over $SOROSWAP_ROUTER"
  SOROSWAP_ID="$(stellar contract deploy --wasm "$(wasm soroswap_adapter)" "${NET[@]}" -- \
    --aggregator "$DEX_ID" --router "$SOROSWAP_ROUTER")"
  invoke "$DEX_ID" allow_venue --venue "$SOROSWAP_ID" --allowed true
  ok "soroswap_adapter -> $SOROSWAP_ID"
fi
AQUARIUS_ID=""
if [ "${AQUARIUS:-0}" = "1" ]; then
  log "Deploying the Aquarius adapter"
  AQUARIUS_ID="$(stellar contract deploy --wasm "$(wasm aquarius_adapter)" "${NET[@]}" -- \
    --aggregator "$DEX_ID")"
  invoke "$DEX_ID" allow_venue --venue "$AQUARIUS_ID" --allowed true
  ok "aquarius_adapter -> $AQUARIUS_ID"
fi

log "Deploying the push oracle (issuer NAV)"
PUSH_ID="$(stellar contract deploy --wasm "$(wasm push_oracle)" "${NET[@]}" -- \
  --admin "$ADMIN_ADDR" --max_deviation_bps "${MAX_DEVIATION_BPS:-200}")"
invoke "$PUSH_ID" set_updater --updater "${KEEPER:-$ADMIN_ADDR}" --allowed true
ok "push_oracle -> $PUSH_ID (updater ${KEEPER:-$ADMIN_ADDR})"

json_merge "$DEPLOYMENT_FILE" \
  '.contracts.pushOracle=$push | .contracts.facilityAggregator=$fac | .contracts.dexAggregator=$dex
   | .contracts.soroswapAdapter=$sw | .contracts.aquariusAdapter=$aq
   | .wasmHashes={vault:$vh, facility:$fh, queue:$qh, sep56Adapter:$sh, blendAdapter:$blh, defindexAdapter:$dfh, mockVenue:$mh}' \
  --arg push "$PUSH_ID" --arg fac "$FAC_ID" --arg dex "$DEX_ID" --arg sw "$SOROSWAP_ID" --arg aq "$AQUARIUS_ID" \
  --arg vh "$VAULT_HASH" --arg fh "$FACILITY_HASH" --arg qh "$QUEUE_HASH" --arg sh "$SEP56_HASH" --arg blh "$BLEND_HASH" --arg dfh "$DEFINDEX_HASH" --arg mh "$MOCK_VENUE_HASH"
ok "wrote $DEPLOYMENT_FILE"
