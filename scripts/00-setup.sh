#!/usr/bin/env bash
# Generate and fund the identities used by the project (admin, maker, taker)
# and record their addresses + secrets in deployments/accounts.<network>.json.
#
# The secrets are written so downstream projects (Octarine-UI / mystic-backend)
# can load them. This is fine for testnet development — NEVER commit real
# mainnet secrets (accounts.*.json is git-ignored).
source "$(dirname "$0")/lib.sh"
require_tools

IDENTITIES=(rfq-admin rfq-maker rfq-taker)

log "Setting up identities on '$NETWORK'"

for name in "${IDENTITIES[@]}"; do
  if stellar keys address "$name" >/dev/null 2>&1; then
    ok "identity '$name' already exists"
  else
    if [ "$FRIENDBOT" = "1" ]; then
      stellar keys generate "$name" --network "$NETWORK" --fund >/dev/null
    else
      stellar keys generate "$name" >/dev/null
      warn "generated '$name' (no friendbot on $NETWORK — fund it manually)"
    fi
    ok "created identity '$name'"
  fi
done

# Make sure testnet accounts are funded (friendbot is idempotent-ish).
if [ "$FRIENDBOT" = "1" ]; then
  for name in "${IDENTITIES[@]}"; do
    stellar keys fund "$name" --network "$NETWORK" >/dev/null 2>&1 || true
  done
fi

ADMIN_ADDR="$(stellar keys address rfq-admin)"
MAKER_ADDR="$(stellar keys address rfq-maker)"
TAKER_ADDR="$(stellar keys address rfq-taker)"
ADMIN_SECRET="$(stellar keys show rfq-admin)"
MAKER_SECRET="$(stellar keys show rfq-maker)"
TAKER_SECRET="$(stellar keys show rfq-taker)"

json_merge "$ACCOUNTS_FILE" \
  '{network:$net, admin:{name:"rfq-admin",address:$aa,secret:$as},
    maker:{name:"rfq-maker",address:$ma,secret:$ms},
    taker:{name:"rfq-taker",address:$ta,secret:$ts}}' \
  --arg net "$NETWORK" \
  --arg aa "$ADMIN_ADDR" --arg as "$ADMIN_SECRET" \
  --arg ma "$MAKER_ADDR" --arg ms "$MAKER_SECRET" \
  --arg ta "$TAKER_ADDR" --arg ts "$TAKER_SECRET"

ok "wrote $ACCOUNTS_FILE"
log "admin: $ADMIN_ADDR"
log "maker: $MAKER_ADDR"
log "taker: $TAKER_ADDR"
