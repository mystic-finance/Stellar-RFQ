/**
 * Deploys the testnet demo facilities: RWA test tokens priced by the push
 * oracle, three USDC facilities (one with two RWAs, one with three), their
 * venues, LP capital and a first trade each. Records everything under
 * `.facilities` in deployments/testnet.json.
 *
 *   ADMIN_SECRET=$(stellar keys show rfq-admin) MAKER_SECRET=$(stellar keys show rfq-maker) \
 *   TAKER_SECRET=$(stellar keys show rfq-taker) npx tsx scripts/deploy-facilities.ts
 */
import { Keypair } from "@stellar/stellar-sdk";
import { readFileSync, writeFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { Buffer } from "buffer";
import { Octarine, keypairSigner, networks, submit, formatUnits, type facility as F } from "../src/index.js";

const NET = networks.testnet;
const FILE = new URL("../../deployments/testnet.json", import.meta.url);
const d = JSON.parse(readFileSync(FILE, "utf8"));
const USDC: string = d.external.usdc;
const DAY = 86_400;
const ONE = 10n ** 18n;

const kp = (k: string) => Keypair.fromSecret(process.env[k] ?? (() => { throw new Error(`set ${k}`); })());
const [admin, maker, taker] = ["ADMIN_SECRET", "MAKER_SECRET", "TAKER_SECRET"].map(kp);
const base = new Octarine({ network: "testnet", contracts: d.contracts });
const as = (k: Keypair) => base.as(keypairSigner(k, NET.networkPassphrase));
const A = as(admin), M = as(maker), T = as(taker);
const me = admin.publicKey();
const log = (s: string) => console.log(s);
const send = async <R>(label: string, tx: Parameters<typeof submit<R>>[0]) => {
  const r = await submit(tx);
  log(`  ${label}: ${r.hash}`);
  return r.result as R;
};

/** RWA test tokens: symbol, name, NAV (USDC per token) and how it is priced. */
const TOKENS = [
  { symbol: "tTBILL", name: "Testnet T-Bill Fund", nav: 10_412_000n, source: "admin push (stand-in NAV)" },
  { symbol: "tMMF", name: "Testnet Money Market Fund", nav: 10_000_000n, source: "admin push (stand-in NAV)" },
  { symbol: "tCLO", name: "Testnet AAA CLO", nav: 0n, source: "keeper: Centrifuge JAAA NAV on Ethereum" },
  { symbol: "tCREDIT", name: "Testnet Private Credit", nav: 9_875_000n, source: "admin push (stand-in NAV)" },
] as const;

const rolling = (days: number, cap = 2000) => ({ mode: { tag: "Rolling" as const, values: undefined }, rolling_seconds: days * DAY, next_redemption_at: 0n, cycle_seconds: 0, max_bps_per_day: cap });
const cyclical = (cycleDays: number, cap = 2000) => {
  const now = Math.floor(Date.now() / 1000);
  return { mode: { tag: "Cyclical" as const, values: undefined }, rolling_seconds: 0, next_redemption_at: BigInt(now + 10 * DAY), cycle_seconds: cycleDays * DAY, max_bps_per_day: cap };
};

/** Facilities: each lists RWAs (by symbol) with terms, and the venues its USDC vault lends to. */
const FACILITIES = [
  {
    key: "treasury",
    name: "Octarine Treasury",
    symbol: "oTSY",
    fee: 500,
    venues: ["blend"],
    rwas: [
      { symbol: "tTBILL", bid: 300, ask: 150, maxBps: 6000, schedule: rolling(7) },
      { symbol: "tMMF", bid: 150, ask: 50, maxBps: 6000, schedule: rolling(1) },
    ],
  },
  {
    key: "credit",
    name: "Octarine Credit",
    symbol: "oCRD",
    fee: 1000,
    venues: ["blend", "defindex"],
    rwas: [
      { symbol: "tCLO", bid: 800, ask: 400, maxBps: 5000, schedule: rolling(30) },
      { symbol: "tCREDIT", bid: 1200, ask: 600, maxBps: 4000, schedule: cyclical(30, 3000) },
    ],
  },
  {
    key: "diversified",
    name: "Octarine Diversified",
    symbol: "oDIV",
    fee: 1000,
    venues: ["defindex"],
    rwas: [
      { symbol: "tTBILL", bid: 350, ask: 175, maxBps: 4000, schedule: rolling(7) },
      { symbol: "tCLO", bid: 900, ask: 450, maxBps: 4000, schedule: rolling(30) },
      { symbol: "tCREDIT", bid: 1300, ask: 650, maxBps: 3000, schedule: cyclical(30, 3000) },
    ],
  },
];

const out: Record<string, unknown> = { tokens: {}, facilities: {} };

log("== RWA test tokens");
const tokenIds: Record<string, string> = {};
const wasmHash = execFileSync("stellar", ["contract", "upload", "--wasm", "../target/wasm32v1-none/release/test_token.wasm", "--source", "rfq-admin", "--network", "testnet"], { encoding: "utf8" }).trim();
for (const t of TOKENS) {
  const id = execFileSync("stellar", ["contract", "deploy", "--wasm-hash", wasmHash, "--source", "rfq-admin", "--network", "testnet"], { encoding: "utf8" }).trim();
  tokenIds[t.symbol] = id;
  await send(`${t.symbol} initialize`, A.token(id).initialize({ admin: me, decimal: 7, name: t.name, symbol: t.symbol }));
  for (const who of [taker.publicKey(), maker.publicKey()]) await send(`${t.symbol} mint`, A.token(id).mint({ to: who, amount: 100_000_0000000n }));
  log(`${t.symbol} ${id}`);
}
out.tokens = tokenIds;

log("== Prices on the push oracle");
const now = Math.floor(Date.now() / 1000) - 10;
for (const t of TOKENS) {
  if (t.nav === 0n) {
    const env = { ...process.env, PUSH_ORACLE: d.contracts.pushOracle, KEEPER_SECRET: admin.secret(), NAV_BASE: tokenIds[t.symbol], NAV_QUOTE: USDC, NAV_BASE_DECIMALS: "7", NAV_QUOTE_DECIMALS: "7" };
    log(execFileSync("npx", ["tsx", "src/index.ts", "--once"], { cwd: new URL("../../keeper/", import.meta.url), env, encoding: "utf8" }).trim());
  } else {
    // 7-dp NAV → quote per base, raw units ×1e18 (both tokens 7 dp).
    await send(`push ${t.symbol}`, A.pushOracle().push({ caller: me, base: tokenIds[t.symbol], quote: USDC, price: (t.nav * ONE) / 10_000_000n, timestamp: BigInt(now) }));
  }
}

log("== Facilities");
const vcfg = (asset: string, sym: string) => ({ asset, curator: me, manager: me, guardian: me, name: sym, symbol: sym, deposit_cap: 0n, curator_fee_bps: 0, timelock: 0n });
const venueOf = { blend: [d.wasmHashes.blendAdapter, d.external.blendPoolV2], defindex: [d.wasmHashes.defindexAdapter, d.external.defindexUsdcVault] } as const;
const agg = A.facilityAggregator();
for (const spec of FACILITIES) {
  log(`-- ${spec.name}`);
  const usdcVault = await send<string>("create USDC vault", agg.create_vault({ config: vcfg(USDC, `${spec.symbol}-USDC`) }));
  const facility = await send<string>("create facility", agg.create_facility({
    config: { base_vault: usdcVault, curator: me, manager: me, guardian: me, name: spec.name, symbol: spec.symbol, curator_fee_bps: spec.fee, vest_seconds: BigInt(3 * DAY), timelock: 0n },
  }));
  const v = A.vault(usdcVault);
  for (const venue of spec.venues) {
    const [hash, target] = venueOf[venue as "blend"];
    await send(`venue ${venue}`, v.submit({ change: { tag: "Venue", values: [Buffer.from(hash, "hex"), target] } }));
    await send(`accept ${venue}`, v.accept({ kind: 2 }));
  }
  const f = A.facility(facility);
  const rwaVaults: Record<string, string> = {};
  for (const r of spec.rwas) {
    const rv = await send<string>(`create ${r.symbol} vault`, agg.create_vault({ config: vcfg(tokenIds[r.symbol], `${spec.symbol}-${r.symbol}`) }));
    rwaVaults[r.symbol] = rv;
    const terms: F.Terms = { accepted: true, bid_rate: r.bid, ask_rate: r.ask, max_bps: r.maxBps, oracle: d.contracts.pushOracle, max_price_age: BigInt(30 * DAY), redemption: me, schedule: r.schedule };
    const [kind] = await send<readonly [number, bigint]>(`list ${r.symbol}`, f.submit({ change: { tag: "Rwa", values: [tokenIds[r.symbol], rv, terms] } }));
    await send(`accept ${r.symbol}`, f.accept({ kind }));
  }
  const queue = await send<string>("create queue", agg.create_queue({ config: { facility, curator: me } }));
  const [qk] = await send<readonly [number, bigint]>("submit queue", f.submit({ change: { tag: "Queue", values: [queue] } }));
  await send("accept queue", f.accept({ kind: qk }));
  await send("set active", agg.set_active({ facility, active: true }));

  // LP capital from two depositors, a slice lent out, and a first sale of each RWA.
  for (const [who, k, want] of [[M, maker, 120_0000000n], [T, taker, 80_0000000n]] as const) {
    const held = (await who.token(USDC).balance({ id: k.publicKey() })).result;
    const amt = held < want ? held : want;
    if (amt <= 0n) continue;
    await send(`deposit ${formatUnits(amt, 7)} USDC`, who.facility(facility).deposit({ assets: amt, receiver: k.publicKey(), from: k.publicKey(), operator: k.publicKey() }));
  }
  const venues = (await A.vault(usdcVault).venues()).result;
  for (const adapter of venues) await send("allocate", v.allocate({ adapter, amount: 30_0000000n }));
  for (const r of spec.rwas) {
    const q = (await A.facilityAggregator().quote_all({ token_in: tokenIds[r.symbol], token_out: USDC, amount_in: 20_0000000n })).result.find((x) => x.facility === facility);
    if (q) await send(`sell 20 ${r.symbol}`, T.facilityAggregator().swap({ payer: taker.publicKey(), recipient: taker.publicKey(), token_in: tokenIds[r.symbol], token_out: USDC, amount_in: 20_0000000n, min_out: q.amount_out, facility }));
  }
  const nav = (await A.facility(facility).total_assets()).result;
  log(`   NAV ${formatUnits(nav, 7)} USDC`);
  (out.facilities as Record<string, unknown>)[spec.key] = { name: spec.name, facility, usdcVault, rwaVaults, venues, queue, rwas: spec.rwas.map((r) => r.symbol) };
}

const latest = JSON.parse(readFileSync(FILE, "utf8"));
latest.rwaTokens = { ...(latest.rwaTokens ?? {}), ...tokenIds };
latest.facilities = { ...(latest.facilities ?? {}), ...(out.facilities as object) };
writeFileSync(FILE, JSON.stringify(latest, null, 2) + "\n");
log("wrote deployments/testnet.json (.rwaTokens, .facilities)");
