#!/usr/bin/env bash
# Testnet demo fixtures — NOT part of deploying the protocol.
# Deploys two custom test tokens, mints balances to maker/taker, and registers
# the maker's order-signing key, then records these under the deployment JSON so
# the TypeScript demo can run. Skip this entirely on mainnet.
source "$(dirname "$0")/lib.sh"
require_tools

[ "$FRIENDBOT" = "0" ] && die "seed-demo is for test networks only (NETWORK=$NETWORK looks like mainnet)."
[ -f "$DEPLOYMENT_FILE" ] || die "Deploy the protocol first: ./scripts/02-deploy.sh"
[ -f "$ACCOUNTS_FILE" ] || die "Run ./scripts/00-setup.sh first."

RFQ_ID="$(deployment .contracts.rfq)"
ADMIN_ADDR="$(account .admin.address)"
MAKER_ADDR="$(account .maker.address)"
TAKER_ADDR="$(account .taker.address)"

TOKEN_WASM="$WASM_RELEASE/test_token.wasm"
[ -f "$TOKEN_WASM" ] || die "No test_token WASM. Build first: ./scripts/01-build.sh"

NET=(--rpc-url "$RPC_URL" --network-passphrase "$NETWORK_PASSPHRASE")
MINT_AMOUNT="${MINT_AMOUNT:-1000000000000}" # 100,000.0000000 (7 decimals)

deploy_token() {
  local code="$1"
  log "Deploying test token $code"
  local id
  id="$(stellar contract deploy --wasm "$TOKEN_WASM" --source rfq-admin "${NET[@]}")"
  stellar contract invoke --id "$id" --source rfq-admin "${NET[@]}" \
    -- initialize --admin "$ADMIN_ADDR" --decimal 7 --name "$code" --symbol "$code" >/dev/null
  ok "$code -> $id"
  echo "$id"
}

TOKEN_A_ID="$(deploy_token RFQA)"
TOKEN_B_ID="$(deploy_token RFQB)"

mint() {
  stellar contract invoke --id "$1" --source rfq-admin "${NET[@]}" \
    -- mint --to "$2" --amount "$MINT_AMOUNT" >/dev/null
}

log "Minting test balances"
mint "$TOKEN_A_ID" "$MAKER_ADDR"   # maker sells token A
mint "$TOKEN_B_ID" "$TAKER_ADDR"   # taker pays token B
ok "minted $MINT_AMOUNT RFQA to maker and RFQB to taker"

log "Registering maker order-signing key"
MAKER_PUBKEY_HEX="$(node "$ROOT_DIR/scripts/strkey.mjs" "$MAKER_ADDR")"
stellar contract invoke --id "$RFQ_ID" --source rfq-maker "${NET[@]}" \
  -- register_order_signer --maker "$MAKER_ADDR" \
     --signer "$MAKER_PUBKEY_HEX" --allowed true >/dev/null
ok "registered $MAKER_PUBKEY_HEX"

json_merge "$DEPLOYMENT_FILE" \
  '.contracts.tokenA=$ta | .contracts.tokenB=$tb
   | .tokens={RFQA:$ta, RFQB:$tb}
   | .accounts={admin:$admin, maker:$maker, taker:$taker}
   | .makerSignerHex=$pub' \
  --arg ta "$TOKEN_A_ID" --arg tb "$TOKEN_B_ID" \
  --arg admin "$ADMIN_ADDR" --arg maker "$MAKER_ADDR" --arg taker "$TAKER_ADDR" \
  --arg pub "$MAKER_PUBKEY_HEX"

ok "updated $DEPLOYMENT_FILE with demo fixtures"
jq . "$DEPLOYMENT_FILE"
