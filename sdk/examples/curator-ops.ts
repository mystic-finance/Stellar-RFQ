/**
 * Example operations bot for a facility curator/manager. Each tick it:
 *  1. serves the withdrawal queue with whatever cash is free;
 *  2. keeps the base vault's idle cash near a target share of its assets,
 *     allocating the surplus to the first venue or pulling a shortfall back;
 *  3. warns when an RWA's price is close to going stale (then the facility
 *     stops quoting it).
 *
 *   MANAGER_SECRET=S... FACILITY=C... IDLE_BPS=2000 npx tsx examples/curator-ops.ts
 */
import { Keypair } from "@stellar/stellar-sdk";
import { Octarine, keypairSigner, networks, submit, formatUnits } from "../src/index.js";

const env = (k: string, d?: string) => {
  const v = process.env[k] ?? d;
  if (v === undefined) throw new Error(`set ${k}`);
  return v;
};
const network = env("NETWORK", "testnet") as "testnet" | "mainnet";
const manager = Keypair.fromSecret(env("MANAGER_SECRET"));
const FACILITY = env("FACILITY");
const IDLE_BPS = BigInt(env("IDLE_BPS", "2000")); // keep 20% of the base vault idle
const BAND_BPS = BigInt(env("BAND_BPS", "500")); // act only beyond ±5 points
const sdk = new Octarine({ network }).as(keypairSigner(manager, networks[network].networkPassphrase));
const fmt = (v: bigint) => formatUnits(v, 7);

async function tick() {
  const f = sdk.facility(FACILITY);
  const cfg = (await f.config()).result;
  const vault = sdk.vault(cfg.base_vault);
  const asset = (await vault.query_asset()).result;

  // 1. Queue
  const q = (await f.queue()).result;
  if (q) {
    const served = (await submit(sdk.queue(q).process({ max: 10 }))).result;
    if (served) console.log(`queue: served ${served}`);
  }

  // 2. Idle buffer on the base vault
  const total = (await vault.total_assets()).result;
  const idle = (await sdk.token(asset).balance({ id: cfg.base_vault })).result;
  const target = (total * IDLE_BPS) / 10_000n;
  const band = (total * BAND_BPS) / 10_000n;
  const venues = (await vault.venues()).result;
  if (venues.length && idle > target + band) {
    const amount = idle - target;
    await submit(vault.allocate({ adapter: venues[0], amount }));
    console.log(`allocated ${fmt(amount)} to ${venues[0]}`);
  } else if (venues.length && idle + band < target) {
    let need = target - idle;
    for (const v of venues) {
      if (need <= 0n) break;
      const avail = (await sdk.venue(v).max_withdraw()).result;
      const take = avail < need ? avail : need;
      if (take > 0n) {
        await submit(vault.deallocate({ adapter: v, amount: take }));
        console.log(`pulled ${fmt(take)} from ${v}`);
        need -= take;
      }
    }
  }

  // 3. Price freshness
  const now = BigInt(Math.floor(Date.now() / 1000));
  for (const rwa of (await f.rwas()).result) {
    const r = (await f.rwa({ rwa })).result;
    if (!r) continue;
    const p = await sdk.pushOracle(r.terms.oracle).get_price({ base: rwa, quote: asset }).catch(() => undefined);
    const age = p ? now - p.result.timestamp : undefined;
    if (age === undefined || age > (r.terms.max_price_age * 8n) / 10n) {
      console.warn(`price for ${rwa} is ${age === undefined ? "missing" : `${age}s old`} (limit ${r.terms.max_price_age}s)`);
    }
  }
  console.log(`${new Date().toISOString()} NAV ${fmt((await f.total_assets()).result)}, idle ${fmt(idle)} / target ${fmt(target)}`);
}

for (;;) {
  await tick().catch((e) => console.error(`tick failed: ${e instanceof Error ? e.message : e}`));
  if (process.argv.includes("--once")) break;
  await new Promise((r) => setTimeout(r, Number(env("INTERVAL_SECONDS", "300")) * 1000));
}
