#!/usr/bin/env python3
"""Builds e2e/fixtures/testnet.json: a ledger snapshot of the deployed testnet
CCTP V2 and LayerZero V2 contracts that the forked-state tests load.

`stellar snapshot create` reads the live bucket list from the history archive,
which holds the contracts' storage but (as of CLI 28 on a protocol 29 network)
writes neither their wasm nor accounts, and cannot see archived entries. So:

1. `stellar snapshot create` for every contract's data;
2. RPC `getLedgerEntries` for each contract's instance and wasm, the accounts
   fees and tokens move to, and the archived LayerZero DVN's entries (an
   archived entry is marked live, as the restore a real transaction would
   do first);
3. pruning: the SAC balance tables (thousands of holders) are cut to the
   addresses here, and used nonces and temporary entries are dropped.

Usage: STELLAR=/path/to/stellar python3 -I e2e/snapshot.py [out.json]
Needs a stellar CLI that knows the network's protocol (28+ for protocol 29).
"""

import base64
import json
import os
import subprocess
import sys
import tempfile
import urllib.request

RPC = os.environ.get("STELLAR_RPC_URL", "https://soroban-testnet.stellar.org")
CLI = os.environ.get("STELLAR", "stellar")
HERE = os.path.dirname(os.path.abspath(__file__))
OUT = sys.argv[1] if len(sys.argv) > 1 else os.path.join(HERE, "fixtures", "testnet.json")

# Circle CCTP V2 (domain 27) and the USDC it mints.
CCTP = {
    "CDNG7HXAPBWICI2E3AUBP3YZWZELJLYSB6F5CC7WLDTLTHVM74SLRTHP": "TokenMessengerMinterV2",
    "CBJ6MTCKKZG73PMDZCJMSFRD7DQEMI4FKDH7CGDSV4W6FHCRBCQAVVJY": "MessageTransmitterV2",
    "CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA": "USDC SAC",
    "CCELIKMY7RQ3BQERWSOQHLIVYC5E3UHLTLDYIO2NVS5XGFEXYER5UWSB": "USDC admin (swap minter)",
    "CBB543RNAI7QXJS3HJKDQ6FXC2AQDSZP2VEWSQMDRUSCYDORVBRHG7OX": "USDCALLCCTP SAC (minter allowance)",
}
# LayerZero V2 (EID 40600): the endpoint, its default library and workers.
LZ = {
    "CALTBA5S6GRJEHAXFP45LGGLKWWAF7HTZCPNUBUJF2HWWRRLQNV35AIV": "EndpointV2",
    "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC": "XLM SAC",
    "CCMLPCAWCPIIMXOHJJKU3NZLOFTT2O6QTB2UUFPN6SEHLK35QRHVKKMB": "ULN302",
    "CCTE7D7O6BDSZILNYKYDXZHOYPXFVEQ3WKGNSELA3G7W3K2T2BOBUU36": "Treasury",
    "CCAVZ7ESAV3PDJ6PISRCXVZCXFFH4NGI7K7MVMZZDR33LC5AD3UPZATT": "Executor",
    "CBB3SROBFLJGD4O4BPVJ6YNZFXTA6BKLPM4T2QKYUBUXMAHOP642LKNW": "Executor fee lib",
    "CAIKTIJD4APTPPLJ6K5XKLNGQBXX7JA4CZJLIBP7YQUC6B54TD6EZVK7": "Executor price feed",
    "CANJCMHRRXEBM46675TSPJIFUVPW7PDOCBLMWS7QPO5TCBHBL7JC4CDL": "Executor helper",
    # The LayerZero Labs DVN for ULN302, which OApps on testnet configure.
    "CB7EWGPRGD3R3CD2DIYIVXMOEFDINVBGBFFQLFMEMXFWJFJIMHA7ZLSA": "DVN (LayerZero Labs)",
    "CCNLXA7DYMUERWQS2GS3JRFIXRCQL7Q76I6LGTLTD3ROOQIMUFQ5NI5G": "DVN (LayerZero Labs) fee lib",
}
# The ULN's default DVN and its fee lib and price feed: archived on testnet, and
# registered for an older ULN, so the default pathway fails (see the tests).
DVN = "CBZ4QEWVC7EGNT5KDQAMPKP5PGIUEKONDEB767MOQSZDFPPLM6Q6LQQ3"
ARCHIVED = {
    DVN: "DVN",
    "CASRGIXQURTTXJB6BLWYW6KYCPBKK7MVHL6MTBFQCFLPBJK7M5TVI55S": "DVN fee lib",
    "CBWLREV4VAKBP5JQJO7UM4L6QMJPNXHWYCQ3HIBNNVI5S6CECOZVAS2Y": "DVN price feed",
}
ACCOUNTS = [
    "GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5",  # USDC issuer
    "GBKCGIEERYEW2XE3KWVI3AS2NZD3HWJWF5LI7HIYZ4KMCULPS4GXQLVZ",  # USDCALLCCTP issuer
    "GCRN5PMAG5FM5QLCH7BUZPRQ7UIW37LBZLF2BIDEOSG4ZQ6HYRC45ALA",  # executor and DVN fee deposit
    "GBRHLONDZBDTL2XCYHVXSE65HSM264RE6FCC5ZKDJ7B3EIPQHITT2M2Y",  # CCTP fee recipient
]
# EVM pathways the tests use: Sepolia (EID 40161, CCTP domain 0), Base Sepolia.
EIDS = [40161, 40245]
SACS = {
    "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
    "CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA",
    "CBB543RNAI7QXJS3HJKDQ6FXC2AQDSZP2VEWSQMDRUSCYDORVBRHG7OX",
}
KEEP = set(CCTP) | set(LZ) | set(ARCHIVED) | set(ACCOUNTS)
LIVE = 0xFFFFFFFF


def sh(args, stdin=None):
    return subprocess.run(args, input=stdin, capture_output=True, text=True, check=True).stdout


def key_xdr(key):
    return sh([CLI, "xdr", "encode", "--type", "LedgerKey"], json.dumps(key)).strip()


def data_json(b64):
    return json.loads(sh([CLI, "xdr", "decode", "--type", "LedgerEntryData", "--output", "json"], b64))


def rpc_entries(keys):
    out = []
    for i in range(0, len(keys), 100):
        body = {"jsonrpc": "2.0", "id": 1, "method": "getLedgerEntries",
                "params": {"keys": keys[i:i + 100]}}
        req = urllib.request.Request(RPC, json.dumps(body).encode(), {"content-type": "application/json", "user-agent": "stellar-rfq-e2e"})
        with urllib.request.urlopen(req) as r:
            out += json.load(r)["result"].get("entries") or []
    return out


def instance(c):
    return {"contract_data": {"contract": c, "key": "ledger_key_contract_instance", "durability": "persistent"}}


def persistent(c, *parts):
    return {"contract_data": {"contract": c, "key": {"vec": list(parts)}, "durability": "persistent"}}


def sym(s):
    return {"symbol": s}


def addresses_in(v):
    """Every address inside a JSON ScVal."""
    if isinstance(v, dict):
        if "address" in v and isinstance(v["address"], str):
            yield v["address"]
        for x in v.values():
            yield from addresses_in(x)
    elif isinstance(v, list):
        for x in v:
            yield from addresses_in(x)


def keep(e):
    d = e["entry"]["data"]
    if "contract_data" not in d:
        return True
    cd = d["contract_data"]
    if cd["durability"] == "temporary":
        return False
    k = cd["key"]
    if isinstance(k, dict) and "vec" in k and k["vec"] and k["vec"][0].get("symbol") in ("UsedNonce", "UsedHash"):
        return False  # replay guards of past messages
    if cd["contract"] in SACS and k != "ledger_key_contract_instance":
        return any(a in KEEP for a in addresses_in(k))
    return True


def main():
    contracts = list(CCTP) + list(LZ)
    raw = os.environ.get("RAW")  # reuse an archive snapshot already taken
    with tempfile.TemporaryDirectory() as tmp:
        if not raw:
            raw = os.path.join(tmp, "raw.json")
            args = [CLI, "snapshot", "create", "--network", "testnet", "--out", raw]
            for c in contracts:
                args += ["--address", c]
            subprocess.run(args, check=True)
        with open(raw) as f:
            snap = json.load(f)

    entries = [e for e in snap["ledger_entries"] if keep(e)]

    extra = [instance(c) for c in contracts + list(ARCHIVED)]
    extra += [{"account": {"account_id": a}} for a in ACCOUNTS]
    extra += [persistent(DVN, sym("Admins")), persistent(DVN, sym("MessageLibs"))]
    for eid in EIDS:
        extra.append(persistent(DVN, sym("DstConfig"), {"u32": eid}))
        extra.append(persistent("CBWLREV4VAKBP5JQJO7UM4L6QMJPNXHWYCQ3HIBNNVI5S6CECOZVAS2Y",
                                sym("DefaultModelPrice"), {"u32": eid % 30000}))
    fetched = rpc_entries([key_xdr(k) for k in extra])
    seq = snap["sequence_number"]

    def entry(f):
        live = f.get("liveUntilLedgerSeq")
        return {"entry": {"last_modified_ledger_seq": f["lastModifiedLedgerSeq"],
                          "data": data_json(f["xdr"]), "ext": "v0"},
                "live_until": LIVE if live is None or live < seq else live}

    by_key = {}
    for e in entries:
        by_key[json.dumps(ledger_key_of(e), sort_keys=True)] = e
    wasms = set()
    for f in fetched:
        e = entry(f)
        by_key[json.dumps(ledger_key_of(e), sort_keys=True)] = e
        cd = e["entry"]["data"].get("contract_data")
        if cd and cd["key"] == "ledger_key_contract_instance":
            exe = cd["val"]["contract_instance"]["executable"]
            if isinstance(exe, dict) and "wasm" in exe:
                wasms.add(exe["wasm"])
    for f in rpc_entries([key_xdr({"contract_code": {"hash": h}}) for h in sorted(wasms)]):
        e = entry(f)
        by_key[json.dumps(ledger_key_of(e), sort_keys=True)] = e

    snap["ledger_entries"] = sorted(by_key.values(), key=lambda e: json.dumps(ledger_key_of(e), sort_keys=True))
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, "w") as f:
        json.dump(snap, f, indent=0)
    print(f"ledger {seq} (protocol {snap['protocol_version']}): {len(snap['ledger_entries'])} entries, "
          f"{len(wasms)} wasms -> {OUT}")


def ledger_key_of(e):
    d = e["entry"]["data"]
    (t, v), = d.items()
    if t == "contract_data":
        return {t: {"contract": v["contract"], "key": v["key"], "durability": v["durability"]}}
    if t == "contract_code":
        return {t: v["hash"]}
    if t == "account":
        return {t: v["account_id"]}
    if t == "trustline":
        return {t: [v["account_id"], v["asset"]]}
    return d


if __name__ == "__main__":
    main()
