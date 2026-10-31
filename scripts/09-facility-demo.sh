#!/usr/bin/env bash
# Facility smoke test on a test network: the factory deploys a USDC vault, an
# ORWA vault and a facility over them; the facility lists ORWA with its own
# price feed; an LP deposits; a seller sells ORWA through the facility
# aggregator, the ORWA going straight into its vault. Run after
# 03-seed-demo.sh and 08-deploy-facility.sh. Timelocks are 0 for the demo.
#
#   ./scripts/09-facility-demo.sh
source "$(dirname "$0")/lib.sh"
require_tools
[ "$FRIENDBOT" = "0" ] && die "facility-demo moves real funds on '$NETWORK'. Test networks only."

FAC_AGG="$(deployment .contracts.facilityAggregator)"
RWA_ID="$(deployment .contracts.rwa)"
USD_ID="$(deployment .contracts.usd)"
[ "$FAC_AGG" != "null" ] || die "Run ./scripts/08-deploy-facility.sh first."
[ "$RWA_ID" != "null" ] || die "Run ./scripts/03-seed-demo.sh first."
ADMIN="$(account .admin.address)"
MAKER="$(account .maker.address)"   # the LP
TAKER="$(account .taker.address)"   # the seller

NET=(--rpc-url "$RPC_URL" --network-passphrase "$NETWORK_PASSPHRASE")
inv()  { stellar contract invoke --id "$1" --source "$2" "${NET[@]}" -- "${@:3}"; }
view() { stellar contract invoke --id "$1" --source rfq-admin "${NET[@]}" --send=no -- "${@:2}" 2>/dev/null; }
bal()  { view "$1" balance --id "$2" | tr -d '"'; }
FAILURES=0
expect() {
  if [ "$1" = "$2" ]; then ok "$3 = $1"
  else warn "$3: expected $2, got $1"; FAILURES=$((FAILURES + 1)); fi
}
vault_config() {
  jq -nc --arg a "$1" --arg w "$ADMIN" --arg n "$2" \
    '{asset:$a, curator:$w, manager:$w, guardian:$w, name:$n, symbol:$n,
      deposit_cap:"0", curator_fee_bps:0, timelock:0}'
}

log "Vaults: one per asset (the ORWA vault is the address the issuer allowlists)"
USD_VAULT="$(inv "$FAC_AGG" rfq-admin create_vault --config "$(vault_config "$USD_ID" oUSD)" | tr -d '"')"
RWA_VAULT="$(inv "$FAC_AGG" rfq-admin create_vault --config "$(vault_config "$RWA_ID" oORWA)" | tr -d '"')"
ok "USD vault $USD_VAULT, ORWA vault $RWA_VAULT"

log "Facility over them, issuing oLP"
FAC_CONFIG="$(jq -nc --arg v "$USD_VAULT" --arg w "$ADMIN" \
  '{base_vault:$v, curator:$w, manager:$w, guardian:$w, name:"Octarine LP", symbol:"oLP",
    curator_fee_bps:0, vest_seconds:2592000, timelock:0}')"
FACILITY="$(inv "$FAC_AGG" rfq-admin create_facility --config "$FAC_CONFIG" | tr -d '"')"
ok "facility $FACILITY"

log "ORWA's price feed (a mock at 1:1) and terms: buy at 10.00, sell at 5.00 bps/day, 30-day horizon"
ORACLE="$(stellar contract deploy --wasm "$WASM_RELEASE/mock_oracle.wasm" --source rfq-admin "${NET[@]}" -- \
  --operator "$ADMIN" --price 1000000000000000000)"
TERMS="$(jq -nc --arg o "$ORACLE" --arg r "$ADMIN" '{accepted:true, bid_rate:1000, ask_rate:500, max_bps:5000,
  oracle:$o, max_price_age:3600, redemption:$r,
  schedule:{mode:"Rolling", rolling_seconds:2592000, next_redemption_at:0, cycle_seconds:0, max_bps_per_day:2000}}')"
KIND="$(inv "$FACILITY" rfq-admin submit --change "{\"Rwa\":[\"$RWA_ID\",\"$RWA_VAULT\",$TERMS]}" | jq -r '.[0]')"
inv "$FACILITY" rfq-admin accept --kind "$KIND" >/dev/null
inv "$FAC_AGG" rfq-admin set_active --facility "$FACILITY" --active true >/dev/null
ok "ORWA listed, facility active"

log "The maker deposits 10,000 OUSD for oLP"
inv "$FACILITY" rfq-maker deposit --assets 100000000000 --receiver "$MAKER" --from "$MAKER" --operator "$MAKER" >/dev/null
expect "$(view "$FACILITY" total_assets | tr -d '"')" 100000000000 "facility NAV"

log "The taker sells 1.0 ORWA through the facility aggregator"
QUOTE="$(view "$FAC_AGG" quote_all --token_in "$RWA_ID" --token_out "$USD_ID" --amount_in 10000000 \
  | jq -r --arg f "$FACILITY" '.[] | select(.facility==$f) | .amount_out')"
USD_BEFORE="$(bal "$USD_ID" "$TAKER")"
inv "$FAC_AGG" rfq-taker swap --payer "$TAKER" --recipient "$TAKER" --token_in "$RWA_ID" --token_out "$USD_ID" \
  --amount_in 10000000 --min_out "$QUOTE" --facility "$FACILITY" >/dev/null
expect "$(( $(bal "$USD_ID" "$TAKER") - USD_BEFORE ))" "$QUOTE" "taker OUSD out"
expect "$(bal "$RWA_ID" "$RWA_VAULT")" 10000000 "ORWA in its vault"
for hop in "$FAC_AGG" "$FACILITY"; do
  expect "$(bal "$RWA_ID" "$hop")" 0 "ORWA held by $hop"
done

[ "$FAILURES" -eq 0 ] || die "$FAILURES check(s) failed"
json_merge "$DEPLOYMENT_FILE" '.demo={facility:$f, usdVault:$u, orwaVault:$r, orwaOracle:$o}' \
  --arg f "$FACILITY" --arg u "$USD_VAULT" --arg r "$RWA_VAULT" --arg o "$ORACLE"
ok "facility demo passed"
