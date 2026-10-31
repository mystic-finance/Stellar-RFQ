/**
 * Deploys a facility over Circle USDC: the stable auctions settle in and
 * the only one CCTP moves cross-chain. Lists tTBILL and tMMF on the same terms
 * as Octarine Treasury. No Blend or DeFindex market takes Circle USDC on
 * testnet, so its idle cash goes to the testnet mock venue. Records it under
 * `.facilities.circle` in deployments/testnet.json.
 *
 * The maker and taker need Circle USDC (faucet.circle.com, or the testnet DEX).
 *
 *   ADMIN_SECRET=$(stellar keys show rfq-admin) MAKER_SECRET=$(stellar keys show rfq-maker) \
 *   TAKER_SECRET=$(stellar keys show rfq-taker) npx tsx scripts/deploy-circle-facility.ts
 */
import { Keypair } from "@stellar/stellar-sdk";
import { readFileSync, writeFileSync } from "node:fs";
import { Buffer } from "buffer";
import { Octarine, keypairSigner, networks, submit, formatUnits, type facility as F } from "../src/index.js";

const NET = networks.testnet;
const FILE = new URL("../../deployments/testnet.json", import.meta.url);
const d = JSON.parse(readFileSync(FILE, "utf8"));
const USDC: string = d.crosschain.usdc;
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

const rolling = (days: number, cap = 2000) => ({ mode: { tag: "Rolling" as const, values: undefined }, rolling_seconds: days * DAY, next_redemption_at: 0n, cycle_seconds: 0, max_bps_per_day: cap });

const SPEC = {
  key: "circle",
  name: "Octarine Circle Treasury",
  symbol: "oCTSY",
  fee: 500,
  rwas: [
    { symbol: "tTBILL", bid: 300, ask: 150, maxBps: 6000, schedule: rolling(7) },
    { symbol: "tMMF", bid: 150, ask: 50, maxBps: 6000, schedule: rolling(1) },
  ],
  deposits: [[M, maker, 500_0000000n], [T, taker, 300_0000000n]] as const,
  allocate: 200_0000000n,
};
const tokenIds: Record<string, string> = d.rwaTokens;

log("== Prices against Circle USDC (an existing price is refreshed, not moved)");
const now = BigInt(Math.floor(Date.now() / 1000) - 10);
for (const r of SPEC.rwas) {
  const current = await A.pushOracle().get_price({ base: tokenIds[r.symbol], quote: USDC }).then((x) => x.result.price).catch(() => undefined);
  await send(`push ${r.symbol}`, A.pushOracle().push({ caller: me, base: tokenIds[r.symbol], quote: USDC, price: current ?? ONE, timestamp: now }));
}

log(`== ${SPEC.name}`);
const vcfg = (asset: string, sym: string) => ({ asset, curator: me, manager: me, guardian: me, name: sym, symbol: sym, deposit_cap: 0n, curator_fee_bps: 0, timelock: 0n });
const agg = A.facilityAggregator();
const usdcVault = await send<string>("create USDC vault", agg.create_vault({ config: vcfg(USDC, `${SPEC.symbol}-USDC`) }));
const facility = await send<string>("create facility", agg.create_facility({
  config: { base_vault: usdcVault, curator: me, manager: me, guardian: me, name: SPEC.name, symbol: SPEC.symbol, curator_fee_bps: SPEC.fee, vest_seconds: BigInt(3 * DAY), timelock: 0n },
}));
const v = A.vault(usdcVault);
// The mock venue's `venue` slot is its operator, who can cap its liquidity.
await send("venue mock", v.submit({ change: { tag: "Venue", values: [Buffer.from(d.wasmHashes.mockVenue, "hex"), me] } }));
await send("accept mock", v.accept({ kind: 2 }));
const f = A.facility(facility);
const rwaVaults: Record<string, string> = {};
for (const r of SPEC.rwas) {
  const rv = await send<string>(`create ${r.symbol} vault`, agg.create_vault({ config: vcfg(tokenIds[r.symbol], `${SPEC.symbol}-${r.symbol}`) }));
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
for (const [who, k, want] of SPEC.deposits) {
  const held = (await who.token(USDC).balance({ id: k.publicKey() })).result;
  const amt = held < want ? held : want;
  if (amt <= 0n) continue;
  await send(`deposit ${formatUnits(amt, 7)} USDC`, who.facility(facility).deposit({ assets: amt, receiver: k.publicKey(), from: k.publicKey(), operator: k.publicKey() }));
}
const venues = (await A.vault(usdcVault).venues()).result;
for (const adapter of venues) await send("allocate", v.allocate({ adapter, amount: SPEC.allocate }));
for (const r of SPEC.rwas) {
  const q = (await A.facilityAggregator().quote_all({ token_in: tokenIds[r.symbol], token_out: USDC, amount_in: 20_0000000n })).result.find((x) => x.facility === facility);
  if (q) await send(`sell 20 ${r.symbol}`, T.facilityAggregator().swap({ payer: taker.publicKey(), recipient: taker.publicKey(), token_in: tokenIds[r.symbol], token_out: USDC, amount_in: 20_0000000n, min_out: q.amount_out, facility }));
}
const nav = (await A.facility(facility).total_assets()).result;
log(`   NAV ${formatUnits(nav, 7)} USDC`);

const latest = JSON.parse(readFileSync(FILE, "utf8"));
latest.facilities = { ...(latest.facilities ?? {}), [SPEC.key]: { name: SPEC.name, base: USDC, facility, usdcVault, rwaVaults, venues, queue, rwas: SPEC.rwas.map((r) => r.symbol) } };
writeFileSync(FILE, JSON.stringify(latest, null, 2) + "\n");
log("wrote deployments/testnet.json (.facilities.circle)");
