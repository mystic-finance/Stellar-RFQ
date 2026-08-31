#!/usr/bin/env bash
# Deploy a SEP-40 price adapter for one (base, quote) pair and register it on the
# settlement contract. Run once per pair; re-running deploys a fresh adapter and
# repoints the pair at it.
#
#   ./scripts/04-oracle.sh                                  # testnet XLM/USDC via Reflector
#   FEED=C... BASE=C... QUOTE=C... BASE_SYMBOL=BTC QUOTE_SYMBOL=USDC ./scripts/04-oracle.sh
#
# Env:
#   FEED           SEP-40 feed contract (default: Reflector CEX/DEX on testnet).
#   BASE / QUOTE   the two SEP-41 token contracts the pair settles between.
#   BASE_SYMBOL    feed key for the base leg. Unset => keyed by BASE's address.
#   QUOTE_SYMBOL   feed key for the quote leg. Only read when CROSS=true.
#   CROSS          true when the feed quotes both legs in a third asset (e.g. USD)
#                  and the two must be divided. Default true.
#   INVERT         true when the feed reports quote/base. Default false.
#   MAX_AGE        seconds a feed price stays usable. Default 900.
#   BASE_DECIMALS / QUOTE_DECIMALS   token decimals. Default 7 (Stellar classic).
#   SCHEDULE_SECONDS  rolling redemption horizon to register for BASE, so the RFQ
#                     rate model can quote it. 0 => skip. Default 86400.
#   MAX_BPS_PER_DAY   rate ceiling for that schedule, in hundredths of a basis
#                     point per day. Default 1000 (= 10.00 bps/day).
source "$(dirname "$0")/lib.sh"
require_tools

[ -f "$DEPLOYMENT_FILE" ] || die "Deploy the protocol first: ./scripts/02-deploy.sh"
RFQ_ID="$(deployment .contracts.rfq)"
[ -n "$RFQ_ID" ] && [ "$RFQ_ID" != "null" ] || die "No settlement contract in $DEPLOYMENT_FILE."

SOURCE="${SOURCE:-rfq-admin}"
ADMIN_ADDR="$(stellar keys address "$SOURCE")"

# Reflector's CEX/DEX feed: 14 decimals, base USD, assets keyed by ticker.
# Verify against your network before trusting these defaults.
if [ "$NETWORK" = "testnet" ]; then
  FEED="${FEED:-CCYOZJCOPG34LLQQ7N24YXBM7LL62R7ONMZ3G6WZAAYPB5OYKOMJRN63}"
  BASE="${BASE:-$(stellar contract id asset --asset native --network testnet)}"
  QUOTE="${QUOTE:-$(stellar contract id asset \
    --asset USDC:GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5 --network testnet)}"
  BASE_SYMBOL="${BASE_SYMBOL:-XLM}"
  QUOTE_SYMBOL="${QUOTE_SYMBOL:-USDC}"
fi

: "${FEED:?FEED is required}"
: "${BASE:?BASE is required}"
: "${QUOTE:?QUOTE is required}"
CROSS="${CROSS:-true}"
INVERT="${INVERT:-false}"
MAX_AGE="${MAX_AGE:-900}"
BASE_DECIMALS="${BASE_DECIMALS:-7}"
QUOTE_DECIMALS="${QUOTE_DECIMALS:-7}"
SCHEDULE_SECONDS="${SCHEDULE_SECONDS:-86400}"
MAX_BPS_PER_DAY="${MAX_BPS_PER_DAY:-1000}"

NET=(--rpc-url "$RPC_URL" --network-passphrase "$NETWORK_PASSPHRASE")

# A leg is keyed either by ticker (Other) or by contract address (Stellar).
asset_key() {
  if [ -n "${2:-}" ]; then jq -nc --arg s "$2" '{Other:$s}'
  else jq -nc --arg a "$1" '{Stellar:$a}'; fi
}
BASE_ASSET="$(asset_key "$BASE" "${BASE_SYMBOL:-}")"
QUOTE_ASSET="$(asset_key "$QUOTE" "${QUOTE_SYMBOL:-}")"

FEED_DECIMALS="$(stellar contract invoke --id "$FEED" --source "$SOURCE" "${NET[@]}" --send=no -- decimals)"
log "Feed $FEED reports $FEED_DECIMALS decimals"

ORACLE_WASM="$WASM_RELEASE/oracle.optimized.wasm"
[ -f "$ORACLE_WASM" ] || ORACLE_WASM="$WASM_RELEASE/oracle.wasm"
[ -f "$ORACLE_WASM" ] || die "No oracle WASM at $WASM_RELEASE. Build first: ./scripts/01-build.sh"

log "Deploying SEP-40 adapter for $BASE_SYMBOL/$QUOTE_SYMBOL"
ORACLE_ID="$(stellar contract deploy --wasm "$ORACLE_WASM" --source "$SOURCE" "${NET[@]}")"
ok "oracle -> $ORACLE_ID"

CFG="$(jq -nc \
  --arg source "$FEED" --arg base "$BASE" --arg quote "$QUOTE" \
  --argjson base_asset "$BASE_ASSET" --argjson quote_asset "$QUOTE_ASSET" \
  --argjson cross "$CROSS" --argjson invert "$INVERT" \
  --argjson bd "$BASE_DECIMALS" --argjson qd "$QUOTE_DECIMALS" --argjson age "$MAX_AGE" \
  '{source:$source, base:$base, quote:$quote, base_asset:$base_asset,
    quote_asset:$quote_asset, cross:$cross, base_decimals:$bd,
    quote_decimals:$qd, max_age:$age, invert:$invert}')"

stellar contract invoke --id "$ORACLE_ID" --source "$SOURCE" "${NET[@]}" \
  -- initialize --admin "$ADMIN_ADDR" --cfg "$CFG" >/dev/null
ok "initialized (cross=$CROSS invert=$INVERT max_age=${MAX_AGE}s)"

log "Reading a live price through the adapter"
PRICE="$(stellar contract invoke --id "$ORACLE_ID" --source "$SOURCE" "${NET[@]}" --send=no -- \
  get_price --base "$BASE" --quote "$QUOTE")"
ok "adapter says: $PRICE"

log "Registering the pair on the settlement contract"
stellar contract invoke --id "$RFQ_ID" --source "$SOURCE" "${NET[@]}" \
  -- set_oracle --base "$BASE" --quote "$QUOTE" \
     --cfg "$(jq -nc --arg o "$ORACLE_ID" --argjson age "$MAX_AGE" '{oracle:$o, max_age:$age}')" >/dev/null
ok "set_oracle($BASE_SYMBOL, $QUOTE_SYMBOL) -> $ORACLE_ID"

if [ "$SCHEDULE_SECONDS" != "0" ]; then
  stellar contract invoke --id "$RFQ_ID" --source "$SOURCE" "${NET[@]}" \
    -- set_schedule --caller "$ADMIN_ADDR" --asset "$BASE" \
       --schedule "$(jq -nc --argjson s "$SCHEDULE_SECONDS" --argjson m "$MAX_BPS_PER_DAY" \
         '{mode:"Rolling", rolling_seconds:$s, next_redemption_at:0, cycle_seconds:0, max_bps_per_day:$m}')" >/dev/null
  ok "schedule for $BASE_SYMBOL = ${SCHEDULE_SECONDS}s rolling @ max $(awk "BEGIN{printf \"%.2f\", $MAX_BPS_PER_DAY/100}") bps/day"
fi

log "Settlement price_of($BASE_SYMBOL, $QUOTE_SYMBOL)"
stellar contract invoke --id "$RFQ_ID" --source "$SOURCE" "${NET[@]}" --send=no -- \
  price_of --base "$BASE" --quote "$QUOTE"

json_merge "$DEPLOYMENT_FILE" \
  '.oracles += [{pair:$pair, oracle:$oracle, feed:$feed, base:$base, quote:$quote, maxAge:$age}]' \
  --arg pair "$BASE_SYMBOL/$QUOTE_SYMBOL" --arg oracle "$ORACLE_ID" --arg feed "$FEED" \
  --arg base "$BASE" --arg quote "$QUOTE" --argjson age "$MAX_AGE"

ok "recorded in $DEPLOYMENT_FILE"
