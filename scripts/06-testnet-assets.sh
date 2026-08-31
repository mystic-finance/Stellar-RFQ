#!/usr/bin/env bash
# Onboard the m-token demo asset set on a test network: mUSDC (the reference
# stable), mRWA and mXLM. Deploys the tokens, repoints the reference, registers
# schedules, seeds a backstop price and mints balances.
#
#   ./scripts/06-testnet-assets.sh
#
# Oracles are NOT registered here. Wire them per pair afterwards, e.g.
#   BASE=<mXLM> QUOTE=<mUSDC> BASE_SYMBOL=XLM QUOTE_SYMBOL=USDC \
#     SCHEDULE_SECONDS=0 ./scripts/04-oracle.sh
source "$(dirname "$0")/lib.sh"
require_tools

[ "$FRIENDBOT" = "0" ] && die "demo assets are for test networks only (NETWORK=$NETWORK)."
[ -f "$DEPLOYMENT_FILE" ] || die "Deploy the protocol first: ./scripts/02-deploy.sh"
[ -f "$ACCOUNTS_FILE" ] || die "Run ./scripts/00-setup.sh first."

RFQ_ID="$(deployment .contracts.rfq)"
ADMIN_ADDR="$(account .admin.address)"
MAKER_ADDR="$(account .maker.address)"
TAKER_ADDR="$(account .taker.address)"

TOKEN_WASM="$WASM_RELEASE/test_token.wasm"
[ -f "$TOKEN_WASM" ] || die "No test_token WASM. Build first: ./scripts/01-build.sh"

NET=(--rpc-url "$RPC_URL" --network-passphrase "$NETWORK_PASSPHRASE")
MINT_AMOUNT="${MINT_AMOUNT:-1000000000000}" # 100,000.0000000 at 7 decimals
ONE=1000000000000000000                     # 1.0 in the contract's 1e18 price scale
DAY=86400

deploy_token() {
  local symbol="$1" id
  id="$(stellar contract deploy --wasm "$TOKEN_WASM" --source rfq-admin "${NET[@]}")"
  stellar contract invoke --id "$id" --source rfq-admin "${NET[@]}" \
    -- initialize --admin "$ADMIN_ADDR" --decimal 7 --name "$symbol" --symbol "$symbol" >/dev/null
  ok "$symbol -> $id"
  echo "$id"
}

mint() {
  stellar contract invoke --id "$1" --source rfq-admin "${NET[@]}" \
    -- mint --to "$2" --amount "$MINT_AMOUNT" >/dev/null
}

log "Deploying the m-token set"
MUSDC_ID="$(deploy_token mUSDC)"
MRWA_ID="$(deploy_token mRWA)"
MXLM_ID="$(deploy_token mXLM)"

# The reference is what every pushed price is denominated in. Repointing it bumps
# the price epoch, so it has to happen before any push below.
log "Repointing the reference asset to mUSDC"
stellar contract invoke --id "$RFQ_ID" --source rfq-admin "${NET[@]}" \
  -- set_reference --asset "$MUSDC_ID" >/dev/null
ok "reference = mUSDC ($MUSDC_ID)"

log "Registering schedules"
stellar contract invoke --id "$RFQ_ID" --source rfq-admin "${NET[@]}" \
  -- set_schedule --caller "$ADMIN_ADDR" --asset "$MRWA_ID" \
     --schedule "$(jq -nc --argjson s $((7 * DAY)) \
       '{mode:"Rolling", rolling_seconds:$s, next_redemption_at:0, cycle_seconds:0, max_bps_per_day:1000}')" >/dev/null
ok "mRWA = Rolling 7d @ max 10.00 bps/day"

# Cyclical redemption lands on a date and rolls forward once it passes, so the
# horizon counts down across the cycle instead of staying put.
ANCHOR=$(( $(date -u +%s) + 3 * DAY ))
stellar contract invoke --id "$RFQ_ID" --source rfq-admin "${NET[@]}" \
  -- set_schedule --caller "$ADMIN_ADDR" --asset "$MXLM_ID" \
     --schedule "$(jq -nc --argjson c $((3 * DAY)) --argjson a "$ANCHOR" \
       '{mode:"Cyclical", rolling_seconds:0, next_redemption_at:$a, cycle_seconds:$c, max_bps_per_day:1000}')" >/dev/null
ok "mXLM = Cyclical 3d @ max 10.00 bps/day (first redemption $ANCHOR)"

# mRWA has no feed, so it runs on the keeper backstop until one is registered.
log "Seeding the mRWA backstop price"
stellar contract invoke --id "$RFQ_ID" --source rfq-admin "${NET[@]}" \
  -- push_price --caller "$ADMIN_ADDR" --asset "$MRWA_ID" --new_price "$ONE" >/dev/null
ok "1 mRWA = 1 mUSDC"

log "Minting balances"
mint "$MRWA_ID" "$TAKER_ADDR"
mint "$MXLM_ID" "$TAKER_ADDR"
mint "$MUSDC_ID" "$MAKER_ADDR"
mint "$MUSDC_ID" "$TAKER_ADDR"
ok "minted $MINT_AMOUNT to taker (mRWA, mXLM, mUSDC) and maker (mUSDC)"

# Drop the tokens from earlier deployments. A stale id left in this file is what
# sends a caller at a pair the current contract has never heard of.
json_merge "$DEPLOYMENT_FILE" \
  'del(.contracts.tokenA, .contracts.tokenB, .contracts.rwa, .contracts.usd)
   | del(.tokens)
   | .oracles = []
   | .contracts.mUSDC=$musdc | .contracts.mRWA=$mrwa | .contracts.mXLM=$mxlm
   | .tokens={mUSDC:$musdc, mRWA:$mrwa, mXLM:$mxlm}
   | .referenceAsset=$musdc
   | .accounts={admin:$admin, maker:$maker, taker:$taker}' \
  --arg musdc "$MUSDC_ID" --arg mrwa "$MRWA_ID" --arg mxlm "$MXLM_ID" \
  --arg admin "$ADMIN_ADDR" --arg maker "$MAKER_ADDR" --arg taker "$TAKER_ADDR"

ok "updated $DEPLOYMENT_FILE"
jq '{contracts, tokens, referenceAsset}' "$DEPLOYMENT_FILE"
