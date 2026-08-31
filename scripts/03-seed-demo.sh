#!/usr/bin/env bash
# Testnet demo fixtures — NOT part of deploying the protocol.
# Deploys a demo RWA token, mints balances, registers the maker's signing key,
# and configures the rate model (schedule + backstop price) so orders are
# quotable. Skip this entirely on mainnet.
source "$(dirname "$0")/lib.sh"
require_tools

[ "$FRIENDBOT" = "0" ] && die "seed-demo is for test networks only (NETWORK=$NETWORK looks like mainnet)."
[ -f "$DEPLOYMENT_FILE" ] || die "Deploy the protocol first: ./scripts/02-deploy.sh"
[ -f "$ACCOUNTS_FILE" ] || die "Run ./scripts/00-setup.sh first."

RFQ_ID="$(deployment .contracts.rfq)"
USD_ID="$(deployment .contracts.usd)"
ADMIN_ADDR="$(account .admin.address)"
MAKER_ADDR="$(account .maker.address)"
TAKER_ADDR="$(account .taker.address)"

TOKEN_WASM="$WASM_RELEASE/test_token.wasm"
[ -f "$TOKEN_WASM" ] || die "No test_token WASM. Build first: ./scripts/01-build.sh"

NET=(--rpc-url "$RPC_URL" --network-passphrase "$NETWORK_PASSPHRASE")
MINT_AMOUNT="${MINT_AMOUNT:-1000000000000}" # 100,000.0000000 (7 decimals)
ONE=1000000000000000000                     # 1.0 in the contract's 1e18 price scale

log "Deploying demo RWA token"
RWA_ID="$(stellar contract deploy --wasm "$TOKEN_WASM" --source rfq-admin "${NET[@]}")"
stellar contract invoke --id "$RWA_ID" --source rfq-admin "${NET[@]}" \
  -- initialize --admin "$ADMIN_ADDR" --decimal 7 --name ORWA --symbol ORWA >/dev/null
ok "ORWA -> $RWA_ID"

mint() {
  stellar contract invoke --id "$1" --source rfq-admin "${NET[@]}" \
    -- mint --to "$2" --amount "$MINT_AMOUNT" >/dev/null
}

log "Minting test balances"
mint "$RWA_ID" "$TAKER_ADDR"   # taker sells the RWA
mint "$USD_ID" "$MAKER_ADDR"   # maker pays stablecoin
ok "minted $MINT_AMOUNT ORWA to taker and OUSD to maker"

log "Registering maker order-signing key"
MAKER_PUBKEY_HEX="$(node "$ROOT_DIR/scripts/strkey.mjs" "$MAKER_ADDR")"
stellar contract invoke --id "$RFQ_ID" --source rfq-maker "${NET[@]}" \
  -- register_order_signer --maker "$MAKER_ADDR" \
     --signer "$MAKER_PUBKEY_HEX" --allowed true >/dev/null
ok "registered $MAKER_PUBKEY_HEX"

log "Configuring the rate model"
# Fees 0-10%, keepers rate-limited to one push per 5 min, 7-day Dutch decay.
#
# Backstop prices live for 72h. That is a test-network figure: nothing here runs
# a keeper, so a shorter bound just means every pushed price expires and the
# whole pair stops quoting with NoPrice a few minutes into a session. Set it
# from your keeper's actual cadence before this config goes anywhere real.
stellar contract invoke --id "$RFQ_ID" --source rfq-admin "${NET[@]}" \
  -- set_config --cfg '{"min_fee_bps":0,"max_fee_bps":1000,"fallback_max_age":259200,"max_deviation_bps":1000,"min_push_interval":300,"max_shift_seconds":86400,"decay_seconds":604800}' >/dev/null
# ORWA redeems on a 30-day rolling horizon; makers may quote up to 10 bps/day.
stellar contract invoke --id "$RFQ_ID" --source rfq-admin "${NET[@]}" \
  -- set_schedule --caller "$ADMIN_ADDR" --asset "$RWA_ID" \
     --schedule '{"mode":"Rolling","rolling_seconds":2592000,"next_redemption_at":0,"cycle_seconds":0,"max_bps_per_day":10}' >/dev/null
# Backstop price: 1 ORWA = 1 OUSD until a real feed is registered.
stellar contract invoke --id "$RFQ_ID" --source rfq-admin "${NET[@]}" \
  -- push_price --caller "$ADMIN_ADDR" --asset "$RWA_ID" --new_price "$ONE" >/dev/null
ok "schedule = 30d rolling @ max 10 bps/day, ORWA priced 1:1 against OUSD"

json_merge "$DEPLOYMENT_FILE" \
  '.contracts.rwa=$rwa
   | .tokens={ORWA:$rwa, OUSD:$usd}
   | .accounts={admin:$admin, maker:$maker, taker:$taker}
   | .makerSignerHex=$pub' \
  --arg rwa "$RWA_ID" --arg usd "$USD_ID" \
  --arg admin "$ADMIN_ADDR" --arg maker "$MAKER_ADDR" --arg taker "$TAKER_ADDR" \
  --arg pub "$MAKER_PUBKEY_HEX"

ok "updated $DEPLOYMENT_FILE with demo fixtures"
jq . "$DEPLOYMENT_FILE"
