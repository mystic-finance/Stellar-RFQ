#!/usr/bin/env bash
# End-to-end settlement test against a deployed network. Exercises every fill
# path with real signatures and real token movement, asserting balances after
# each one. Test networks only.
#
#   ./scripts/05-fill-demo.sh
source "$(dirname "$0")/lib.sh"
require_tools
require_cmd node "Install Node 18+."

[ "$FRIENDBOT" = "0" ] && die "fill-demo moves real funds on '$NETWORK'. Test networks only."
[ -f "$DEPLOYMENT_FILE" ] || die "Deploy first: ./scripts/02-deploy.sh"
[ -f "$ACCOUNTS_FILE" ] || die "Run ./scripts/00-setup.sh first."

RFQ_ID="$(deployment .contracts.rfq)"
ROUTER_ID="$(deployment .contracts.router)"
RWA_ID="$(deployment .contracts.rwa)"
USD_ID="$(deployment .contracts.usd)"
[ "$RWA_ID" != "null" ] || die "No demo RWA. Run ./scripts/03-seed-demo.sh first."

ADMIN_ADDR="$(account .admin.address)"
MAKER_ADDR="$(account .maker.address)"
TAKER_ADDR="$(account .taker.address)"
MAKER_SECRET="$(account .maker.secret)"
TAKER_SECRET="$(account .taker.secret)"

NET=(--rpc-url "$RPC_URL" --network-passphrase "$NETWORK_PASSPHRASE")
SIGN="$SCRIPT_DIR/sign.mjs"
FAILURES=0

# Surfaces the contract error code on failure. A swallowed stderr here turns
# every problem into a bare non-zero exit.
inv() {
  local err out rc=0
  err="$(mktemp)"
  out="$(stellar contract invoke --id "$1" --source "$2" "${NET[@]}" -- "${@:3}" 2>"$err")" || rc=$?
  if [ "$rc" -ne 0 ]; then
    warn "$3 failed: $(grep -oE 'Error\(Contract, #[0-9]+\)' "$err" | head -1 || echo "see below")"
    tail -3 "$err" >&2
    rm -f "$err"
    return "$rc"
  fi
  rm -f "$err"
  printf '%s' "$out"
}
view() { stellar contract invoke --id "$1" --source rfq-admin "${NET[@]}" --send=no -- "${@:2}" 2>/dev/null; }
bal()  { view "$1" balance --id "$2" | tr -d '"'; }
unquote() { tr -d '"'; }

expect() {
  if [ "$1" = "$2" ]; then ok "$3 = $1"
  else warn "$3: expected $2, got $1"; FAILURES=$((FAILURES + 1)); fi
}

# The Dutch ask decays every second, so anything measured against it has to be a
# band: a few seconds pass between building a transaction and it landing.
expect_between() {
  if [ "$1" -ge "$2" ] && [ "$1" -le "$3" ]; then ok "$4 = $1 (in [$2, $3])"
  else warn "$4: expected $2..$3, got $1"; FAILURES=$((FAILURES + 1)); fi
}

# --- allowances -------------------------------------------------------------
# Settlement pulls both legs, so both sides approve it. The router additionally
# needs the taker's approval only for open bids, which this script does not use.
LEDGER="$(curl -s -X POST "$RPC_URL" -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"getLatestLedger"}' | jq -r .result.sequence)"
EXPIRY_LEDGER=$((LEDGER + 500000))
BIG=1000000000000

log "Approving the settlement contract on both legs (expires at ledger $EXPIRY_LEDGER)"
inv "$RWA_ID" rfq-taker approve --from "$TAKER_ADDR" --spender "$RFQ_ID" \
  --amount "$BIG" --expiration_ledger "$EXPIRY_LEDGER" >/dev/null
inv "$USD_ID" rfq-maker approve --from "$MAKER_ADDR" --spender "$RFQ_ID" \
  --amount "$BIG" --expiration_ledger "$EXPIRY_LEDGER" >/dev/null
ok "approved"

# The demo backstop expires after `fallback_max_age` (1h in the seed config), and
# without a live price the rate model refuses to quote. A keeper would be pushing
# this continuously; here we push once so the run has a price to work from.
log "Refreshing the ORWA backstop price"
inv "$RFQ_ID" rfq-admin push_price --caller "$ADMIN_ADDR" --asset "$RWA_ID" \
  --new_price 1000000000000000000 >/dev/null
ok "ORWA = 1.0 OUSD"

now()    { date -u +%s; }
expiry() { echo $(( $(now) + 3600 )); }

# --- 1. RFQ order: the maker signs a rate, not an amount --------------------
log "1/5  RFQ fill — 1.0 ORWA at 10 bps/day over a 30-day horizon"
# Monotonic plus a random suffix: a repeat run must never land on a salt an
# earlier run cancelled.
SALT=$(( $(now) * 1000 + RANDOM % 1000 ))
RFQ_ORDER="$(jq -nc \
  --arg mt "$USD_ID" --arg tt "$RWA_ID" --arg fr "$ADMIN_ADDR" --arg mk "$MAKER_ADDR" \
  --argjson e "$(expiry)" --argjson salt "$SALT" \
  '{maker_token:$mt, taker_token:$tt, taker_amount:"10000000", min_received_amount:"9000000",
    fee_bps:0, taker:null, sender:null, fee_recipient:$fr, expiry:$e, salt:$salt,
    taker_max_bps_per_day:10, maker_bps_per_day:10, max_maker_amount:"10000000", maker:$mk}')"

ORDER_HASH="$(view "$RFQ_ID" hash_rfq_order --order "$RFQ_ORDER" | unquote)"
MAKER_SIG="$(node "$SIGN" "$MAKER_SECRET" "$ORDER_HASH")"

RWA_BEFORE="$(bal "$RWA_ID" "$TAKER_ADDR")"; USD_BEFORE="$(bal "$USD_ID" "$TAKER_ADDR")"
inv "$RFQ_ID" rfq-taker fill_rfq_order --order "$RFQ_ORDER" --maker_signature "$MAKER_SIG" \
  --taker_signature null --sender "$TAKER_ADDR" --taker_amount_in 10000000 >/dev/null
expect "$(( RWA_BEFORE - $(bal "$RWA_ID" "$TAKER_ADDR") ))" 10000000 "taker ORWA spent"
expect "$(( $(bal "$USD_ID" "$TAKER_ADDR") - USD_BEFORE ))" 9700000 "taker OUSD received (3% discount)"
expect "$(view "$RFQ_ID" filled_amount --order_hash "$ORDER_HASH" | unquote)" 10000000 "recorded fill"

# --- 2. Fixed order: the maker states the amount outright -------------------
log "2/5  Fixed fill — maker names the amount, no oracle, no schedule"
SALT=$((SALT + 1))
FIXED_ORDER="$(jq -nc \
  --arg mt "$USD_ID" --arg tt "$RWA_ID" --arg fr "$ADMIN_ADDR" --arg mk "$MAKER_ADDR" \
  --argjson e "$(expiry)" --argjson salt "$SALT" \
  '{maker_token:$mt, taker_token:$tt, taker_amount:"10000000", min_received_amount:"1",
    fee_bps:100, taker:null, sender:null, fee_recipient:$fr, expiry:$e, salt:$salt,
    maker_amount:"9500000", maker:$mk}')"

FIXED_HASH="$(view "$RFQ_ID" hash_fixed_order --order "$FIXED_ORDER" | unquote)"
FIXED_SIG="$(node "$SIGN" "$MAKER_SECRET" "$FIXED_HASH")"

USD_BEFORE="$(bal "$USD_ID" "$TAKER_ADDR")"; FEE_BEFORE="$(bal "$USD_ID" "$ADMIN_ADDR")"
inv "$RFQ_ID" rfq-taker fill_fixed_order --order "$FIXED_ORDER" --maker_signature "$FIXED_SIG" \
  --taker_signature null --sender "$TAKER_ADDR" --taker_amount_in 5000000 >/dev/null
# Half the order: 4_750_000 gross, 1% fee = 47_500 to the recipient.
expect "$(( $(bal "$USD_ID" "$TAKER_ADDR") - USD_BEFORE ))" 4702500 "taker OUSD net of fee"
expect "$(( $(bal "$USD_ID" "$ADMIN_ADDR") - FEE_BEFORE ))" 47500 "fee recipient"

# --- 3. Router: a bid quoted to a named taker, submitted by the router ------
log "3/5  Router — signed leg quoted to the taker, routed in one transaction"
SALT=$((SALT + 1))
ROUTED_ORDER="$(jq -nc \
  --arg mt "$USD_ID" --arg tt "$RWA_ID" --arg fr "$ADMIN_ADDR" --arg mk "$MAKER_ADDR" \
  --arg tk "$TAKER_ADDR" --arg sd "$ROUTER_ID" --argjson e "$(expiry)" --argjson salt "$SALT" \
  '{maker_token:$mt, taker_token:$tt, taker_amount:"10000000", min_received_amount:"9000000",
    fee_bps:0, taker:$tk, sender:$sd, fee_recipient:$fr, expiry:$e, salt:$salt,
    taker_max_bps_per_day:10, maker_bps_per_day:10, max_maker_amount:"10000000", maker:$mk}')"

REQUEST="$(jq -nc --argjson o "$ROUTED_ORDER" \
  '{maker_token:$o.maker_token, taker_token:$o.taker_token, taker_amount:$o.taker_amount,
    min_received_amount:$o.min_received_amount, fee_bps:$o.fee_bps, taker:$o.taker,
    sender:$o.sender, fee_recipient:$o.fee_recipient, expiry:$o.expiry, salt:$o.salt,
    taker_max_bps_per_day:$o.taker_max_bps_per_day, order_type:"Rfq"}')"

ROUTED_HASH="$(view "$RFQ_ID" hash_rfq_order --order "$ROUTED_ORDER" | unquote)"
REQUEST_HASH="$(view "$RFQ_ID" hash_request --request "$REQUEST" | unquote)"
ROUTED_MAKER_SIG="$(node "$SIGN" "$MAKER_SECRET" "$ROUTED_HASH")"
ROUTED_TAKER_SIG="$(node "$SIGN" "$TAKER_SECRET" "$REQUEST_HASH")"

ROUTE="$(jq -nc --argjson o "$ROUTED_ORDER" --argjson ms "$ROUTED_MAKER_SIG" \
  --argjson ts "$ROUTED_TAKER_SIG" \
  '[{Rfq:{order:{Rfq:$o}, maker_signature:$ms, taker_signature:[$ts], taker_amount:"10000000"}}]')"

USD_BEFORE="$(bal "$USD_ID" "$TAKER_ADDR")"; RWA_BEFORE="$(bal "$RWA_ID" "$TAKER_ADDR")"
inv "$ROUTER_ID" rfq-taker fill --taker "$TAKER_ADDR" --taker_token "$RWA_ID" \
  --maker_token "$USD_ID" --route "$ROUTE" --min_out 9700000 >/dev/null
expect "$(( $(bal "$USD_ID" "$TAKER_ADDR") - USD_BEFORE ))" 9700000 "routed OUSD out"
expect "$(( RWA_BEFORE - $(bal "$RWA_ID" "$TAKER_ADDR") ))" 10000000 "routed ORWA in"
expect "$(bal "$USD_ID" "$ROUTER_ID")" 0 "router holds no OUSD"
expect "$(bal "$RWA_ID" "$ROUTER_ID")" 0 "router holds no ORWA"

# --- 4. Dutch listing: escrow, decay, fill ----------------------------------
log "4/5  Dutch listing — escrow, decaying ask, fill from the escrow"
DUTCH="$(jq -nc --arg mt "$USD_ID" --arg tt "$RWA_ID" --arg fr "$ADMIN_ADDR" \
  '{maker_token:$mt, taker_token:$tt, taker_amount:"10000000", start_maker_amount:"10000000",
    min_maker_amount:"8000000", fee_bps:0, fee_recipient:$fr, expiry:0}')"

LISTING_ID="$(inv "$RFQ_ID" rfq-taker create_dutch_order --seller "$TAKER_ADDR" --order "$DUTCH" | unquote)"
ok "listing $LISTING_ID created"
expect "$(bal "$RWA_ID" "$RFQ_ID")" 10000000 "escrowed ORWA"
# Start 10_000_000, floor 8_000_000 over a 7-day decay: ~3.3 units/second, so a
# freshly created listing is still within a whisker of its start price.
ASK_AT_START="$(view "$RFQ_ID" current_ask --id "$LISTING_ID" | unquote)"
expect_between "$ASK_AT_START" 9999000 10000000 "ask just after creation"

log "Maker buys the listing at the current ask"
MAKER_RWA_BEFORE="$(bal "$RWA_ID" "$MAKER_ADDR")"; SELLER_USD_BEFORE="$(bal "$USD_ID" "$TAKER_ADDR")"
FILL_RESULT="$(inv "$RFQ_ID" rfq-maker fill_dutch_order --id "$LISTING_ID" --buyer "$MAKER_ADDR" \
  --max_maker_amount "$ASK_AT_START")"
# What the seller was paid must equal what the contract reported charging, at the
# ask of the ledger the fill landed in, not the ask we read a few seconds earlier.
CHARGED="$(echo "$FILL_RESULT" | jq -r .maker_filled)"
expect "$(( $(bal "$RWA_ID" "$MAKER_ADDR") - MAKER_RWA_BEFORE ))" 10000000 "buyer received ORWA"
expect "$(( $(bal "$USD_ID" "$TAKER_ADDR") - SELLER_USD_BEFORE ))" "$CHARGED" "seller paid the charged ask"
expect_between "$CHARGED" 8000000 "$ASK_AT_START" "charged ask inside the curve"
expect "$(bal "$RWA_ID" "$RFQ_ID")" 0 "escrow drained"

# --- 5. Cancellation must actually stop a fill ------------------------------
log "5/5  Salt cancellation — a retracted bid must not settle"
SALT=$((SALT + 1))
CANCELLED="$(jq -nc --argjson o "$RFQ_ORDER" --argjson salt "$SALT" '$o | .salt = $salt')"
CANCELLED_HASH="$(view "$RFQ_ID" hash_rfq_order --order "$CANCELLED" | unquote)"
CANCELLED_SIG="$(node "$SIGN" "$MAKER_SECRET" "$CANCELLED_HASH")"

inv "$RFQ_ID" rfq-maker cancel_salt --caller "$MAKER_ADDR" --signer "$MAKER_ADDR" --salt "$SALT" >/dev/null
expect "$(view "$RFQ_ID" is_salt_cancelled --signer "$MAKER_ADDR" --salt "$SALT")" true "salt marked cancelled"

if stellar contract invoke --id "$RFQ_ID" --source rfq-taker "${NET[@]}" -- \
  fill_rfq_order --order "$CANCELLED" --maker_signature "$CANCELLED_SIG" \
  --taker_signature null --sender "$TAKER_ADDR" --taker_amount_in 1000000 >/dev/null 2>&1
then warn "a cancelled order settled"; FAILURES=$((FAILURES + 1))
else ok "cancelled order refused"; fi

echo
if [ "$FAILURES" -eq 0 ]; then ok "all flows passed"
else die "$FAILURES assertion(s) failed"; fi
