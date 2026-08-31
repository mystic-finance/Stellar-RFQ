#!/usr/bin/env bash
# Point the backend's Stellar token registry at the current deployment.
#
# The registry is what the UI lists and what `getAssetMetadata` prefers over the
# on-chain symbol, so a stale row here offers a pair the settlement contract has
# no schedule or price for, and every request against it fails.
#
#   API=https://staging-api.octarine.finance ./scripts/07-backend-tokens.sh
#   API=http://localhost:3000 DRY_RUN=1 ./scripts/07-backend-tokens.sh   # print only
#
# Env:
#   API      backend base URL (no trailing slash). Required.
#   PRUNE    "1" to delete Stellar rows that are not in the current set.
#   DRY_RUN  "1" to print the payload without sending it.
source "$(dirname "$0")/lib.sh"
require_tools

: "${API:?API=<backend base url> is required}"
CHAIN_ID="${CHAIN_ID:-100101}" # STELLAR_CHAIN_IDS.testnet in the backend
[ "$NETWORK" = "testnet" ] || warn "NETWORK=$NETWORK but CHAIN_ID=$CHAIN_ID; override CHAIN_ID if that is wrong."

tok() { deployment ".tokens.$1"; }
MUSDC="$(tok mUSDC)"; MRWA="$(tok mRWA)"; MXLM="$(tok mXLM)"
for v in "$MUSDC" "$MRWA" "$MXLM"; do
  [ -n "$v" ] && [ "$v" != "null" ] || die "Missing token in $DEPLOYMENT_FILE. Run ./scripts/06-testnet-assets.sh first."
done

# Addresses are stored lowercased (the schema lowercases them, and the Stellar
# code upper-cases on the way back out to the chain).
PAYLOAD="$(jq -nc --argjson c "$CHAIN_ID" \
  --arg musdc "$MUSDC" --arg mrwa "$MRWA" --arg mxlm "$MXLM" \
  '[{chainId:$c, address:$musdc, name:"Mystic USD Coin",  symbol:"mUSDC", decimals:7},
    {chainId:$c, address:$mrwa,  name:"Mystic RWA",       symbol:"mRWA",  decimals:7},
    {chainId:$c, address:$mxlm,  name:"Mystic XLM",       symbol:"mXLM",  decimals:7}]')"

log "Stellar token set for chain $CHAIN_ID"
echo "$PAYLOAD" | jq -r '.[] | "  \(.symbol)  \(.address)"' >&2

if [ "${DRY_RUN:-0}" = "1" ]; then
  echo "$PAYLOAD" | jq .
  exit 0
fi

if [ "${PRUNE:-0}" = "1" ]; then
  log "Pruning Stellar rows outside the current set"
  KEEP="$(echo "$PAYLOAD" | jq -r '.[].address | ascii_downcase' | sort -u)"
  curl -sf "$API/octarine/tokens" \
    | jq -r --argjson c "$CHAIN_ID" '.[] | select(.chainId == $c) | .address' 2>/dev/null \
    | while read -r addr; do
        grep -qix "$addr" <<<"$KEEP" && continue
        curl -sf -X DELETE "$API/octarine/tokens/$CHAIN_ID/$addr" >/dev/null \
          && ok "deleted $addr" || warn "could not delete $addr"
      done
fi

log "Upserting via POST $API/octarine/tokens/bulk"
RESP="$(curl -sf -X POST "$API/octarine/tokens/bulk" \
  -H 'Content-Type: application/json' -d "$PAYLOAD")" \
  || die "bulk upsert failed. Is $API reachable, and is the route unguarded on this deployment?"
echo "$RESP" | jq -c '{upserted: (.upsertedCount // 0), modified: (.modifiedCount // 0)}' 2>/dev/null || echo "$RESP"

log "Registry now reports for chain $CHAIN_ID"
curl -sf "$API/octarine/tokens" \
  | jq -r --argjson c "$CHAIN_ID" '.[] | select(.chainId == $c) | "  \(.symbol)\t\(.address)"' \
  || warn "could not read the list back"
