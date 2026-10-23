#!/usr/bin/env bash
# Deploy the cross-chain settle contracts on Stellar testnet, one per bridge, sharing one fresh RFQ
# and verifier: cctp_settle against Circle CCTP V2, peered with CctpSettle on Ethereum Sepolia
# (domain 0), and lz_settle against the LayerZero V2 endpoint, peered with LzSettle there (EID 40161).
# Testnet only: delay 0, tTBILL as the RWA. Build first (01-build.sh).
#   EVM_CCTP_PEER=<CctpSettle 0x…> EVM_LZ_PEER=<LzSettle 0x…> ./scripts/10-deploy-crosschain.sh
# Records .crosschain in deployments/testnet.json. See docs/DEPLOYMENTS.md.
source "$(dirname "$0")/lib.sh"
require_tools
[ "$NETWORK" = "testnet" ] || die "testnet only"
: "${EVM_CCTP_PEER:?EVM_CCTP_PEER=<CctpSettle address on Sepolia> is required}"
: "${EVM_LZ_PEER:?EVM_LZ_PEER=<LzSettle address on Sepolia> is required}"

SOURCE="${SOURCE:-rfq-admin}"
ADMIN="$(stellar keys address "$SOURCE")"
NET=(--rpc-url "$RPC_URL" --network-passphrase "$NETWORK_PASSPHRASE")
inv() { stellar contract invoke --source "$SOURCE" "${NET[@]}" --id "$@"; }
deploy() { stellar contract deploy --source "$SOURCE" "${NET[@]}" --wasm "$WASM_RELEASE/$1.wasm" "${@:2}"; }
evm_word() { printf '%064s' "${1#0x}" | tr ' ' 0 | tr 'A-F' 'a-f'; }

# Circle CCTP V2 (domain 27) and LayerZero V2 (EID 40600) on Stellar testnet.
USDC=CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA
TRANSMITTER=CBJ6MTCKKZG73PMDZCJMSFRD7DQEMI4FKDH7CGDSV4W6FHCRBCQAVVJY
MESSENGER=CDNG7HXAPBWICI2E3AUBP3YZWZELJLYSB6F5CC7WLDTLTHVM74SLRTHP
ENDPOINT=CALTBA5S6GRJEHAXFP45LGGLKWWAF7HTZCPNUBUJF2HWWRRLQNV35AIV
ULN=CCMLPCAWCPIIMXOHJJKU3NZLOFTT2O6QTB2UUFPN6SEHLK35QRHVKKMB
XLM=CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC
RWA="${RWA:-CDVPXGVQYKZ2JLYIG5RCTTOXUEOBPQAILZYUVNPBVBMC4DYZCCNFI5IR}" # tTBILL
# Ethereum Sepolia: CCTP domain 0, LayerZero EID 40161.
SEPOLIA_DOMAIN=0
SEPOLIA_EID=40161
SEPOLIA_USDC=0x1c7D4B196Cb0C7B01d743Fbc6116a902379C7238
# Type 3 executor options, lzReceive with 1M gas on Sepolia: the ULN rejects empty ones, and
# LzSettle needs 500k left (PAY_GAS) before it pays a FILL, plus the OFT send.
OPTIONS=000301001101000000000000000000000000000f4240
# OAppUlnConfig requiring the LayerZero Labs DVN CB7EWG…ZLSA: the ULN's default DVN is archived
# and bound to an older ULN, so every send on the default pathway fails (error 1213).
ULN_CONFIG=0000001100000001000000040000000f0000000a756c6e5f636f6e66696700000000001100000001000000040000000f0000000d636f6e6669726d6174696f6e730000000000000500000000000000000000000f000000166f7074696f6e616c5f64766e5f7468726573686f6c64000000000003000000000000000f0000000d6f7074696f6e616c5f64766e730000000000001000000001000000000000000f0000000d72657175697265645f64766e7300000000000010000000010000000100000012000000017e4b19f130f71d887a1a308add8e214686d426094b05958465cb64952861c1fc0000000f000000197573655f64656661756c745f636f6e6669726d6174696f6e7300000000000000000000010000000f000000197573655f64656661756c745f6f7074696f6e616c5f64766e7300000000000000000000010000000f000000197573655f64656661756c745f72657175697265645f64766e730000000000000000000000

log "RFQ (reference: Circle USDC) with $RWA priced 1:1 and a 30-day rolling schedule"
RFQ="$(deploy rfq)"
inv "$RFQ" -- initialize --admin "$ADMIN" --reference "$USDC" >/dev/null
inv "$RFQ" -- set_config --cfg '{"min_fee_bps":0,"max_fee_bps":1000,"fallback_max_age":2592000,"max_deviation_bps":5000,"min_push_interval":300,"max_shift_seconds":0,"decay_seconds":604800}' >/dev/null
inv "$RFQ" -- set_schedule --caller "$ADMIN" --asset "$RWA" \
  --schedule '{"mode":"Rolling","rolling_seconds":2592000,"next_redemption_at":0,"cycle_seconds":0,"max_bps_per_day":1000}' >/dev/null
inv "$RFQ" -- push_price --caller "$ADMIN" --asset "$RWA" --new_price 1000000000000000000 >/dev/null
ok "rfq -> $RFQ"

VERIFIER="$(deploy verifier)"
ok "verifier -> $VERIFIER"

govern() {
  inv "$1" -- submit --change "$2" >/dev/null
  inv "$1" -- accept --kind "$(inv "$1" --send=no -- kind_of --change "$2")" >/dev/null
}
peer() { echo "{\"Peer\":[$1,{\"addr\":\"$(evm_word "$2")\",\"return_fee\":\"$3\",\"options\":\"$4\"}]}"; }
settle() { deploy "$1" -- --admin "$ADMIN" --guardian "$ADMIN" --delay 0 --config "$2"; }
config() {
  jq -nc --arg rfq "$RFQ" --arg v "$VERIFIER" --arg x "$XLM" --arg tip "$1" --argjson b "$2" \
    '{rfq:$rfq,verifier:$v,xlm:$x,tip:$tip,bridge:$b}'
}

log "cctp_settle (delay 0; tip 0.50 USDC; the burn home is free, so no return fee)"
BRIDGE=$(jq -nc --arg t "$TRANSMITTER" --arg m "$MESSENGER" '{transmitter:$t,messenger:$m}')
CCTP_SETTLE="$(settle cctp_settle "$(config 500000 "$BRIDGE")")"
govern "$CCTP_SETTLE" "$(peer $SEPOLIA_DOMAIN "$EVM_CCTP_PEER" 0 "")"
govern "$CCTP_SETTLE" "{\"Route\":[$SEPOLIA_DOMAIN,\"$USDC\",\"$(evm_word "$SEPOLIA_USDC")\"]}"
ok "cctp_settle -> $CCTP_SETTLE, peer and USDC route for Sepolia ($EVM_CCTP_PEER)"

# bridge.ofts maps each token to its OFT, e.g. {"<USDT0>":"<its OFT>","<RWA>":"<its OFT>"}; the RWA's
# OFT plus a Route for it enables CROSS_DELIVER.
log "lz_settle (delay 0; tip 0; no USDT0 or RWA OFT on testnet, so no OFT and no route)"
BRIDGE=$(jq -nc --arg e "$ENDPOINT" '{endpoint:$e,ofts:{}}')
LZ_SETTLE="$(settle lz_settle "$(config 0 "$BRIDGE")")"
govern "$LZ_SETTLE" "$(peer $SEPOLIA_EID "$EVM_LZ_PEER" 10000000 "$OPTIONS")"
inv "$ENDPOINT" -- set_config --caller "$ADMIN" --oapp "$LZ_SETTLE" --lib "$ULN" \
  --params "[{\"eid\":$SEPOLIA_EID,\"config_type\":2,\"config\":\"$ULN_CONFIG\"},{\"eid\":$SEPOLIA_EID,\"config_type\":3,\"config\":\"$ULN_CONFIG\"}]" >/dev/null
inv "$XLM" -- transfer --from "$ADMIN" --to "$LZ_SETTLE" --amount 500000000 >/dev/null
ok "lz_settle -> $LZ_SETTLE, peer for Sepolia ($EVM_LZ_PEER), LayerZero Labs DVN, 50 XLM for return legs"

json_merge "$DEPLOYMENT_FILE" \
  '.crosschain = {rfq:$r, verifier:$v, rwa:$rwa, usdc:$u, cctp:{settle:$c, sepolia:{domain:0, settle:$cp}}, lz:{settle:$l, sepolia:{eid:40161, settle:$lp}}}' \
  --arg r "$RFQ" --arg v "$VERIFIER" --arg rwa "$RWA" --arg u "$USDC" \
  --arg c "$CCTP_SETTLE" --arg cp "$EVM_CCTP_PEER" --arg l "$LZ_SETTLE" --arg lp "$EVM_LZ_PEER"
word() { python3 -c "import base64,sys;print('0x'+base64.b32decode(sys.argv[1])[1:33].hex())" "$1"; }
log "Now peer Sepolia with STELLAR_CCTP_PEER=$(word "$CCTP_SETTLE") STELLAR_LZ_PEER=$(word "$LZ_SETTLE")"
