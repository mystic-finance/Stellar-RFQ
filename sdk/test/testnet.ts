/**
 * End-to-end on testnet against real assets and protocols: Soroswap, Aquarius,
 * Blend v2, DeFindex, the router, the facility stack and the push oracle.
 *
 *   ADMIN_SECRET=$(stellar keys show rfq-admin) MAKER_SECRET=$(stellar keys show rfq-maker) \
 *   TAKER_SECRET=$(stellar keys show rfq-taker) npm run test:testnet
 *
 * Writes every transaction hash to ../deployments/testnet-integration.json.
 */
import { Asset, Keypair, Operation, rpc, TransactionBuilder } from "@stellar/stellar-sdk";
import type { AssembledTransaction } from "@stellar/stellar-sdk/contract";
import { readFileSync, writeFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import {
  Octarine,
  keypairSigner,
  networks,
  facilityLegData,
  dexLegData,
  aquariusHopData,
  soroswapHopData,
  sellQuote,
  formatUnits,
  submit,
  rwaChangeKind,
  type facility as F,
} from "../src/index.js";

const NET = networks.testnet;
const DEPLOYMENT = new URL("../../deployments/testnet.json", import.meta.url);
const REPORT = new URL("../../deployments/testnet-integration.json", import.meta.url);
const d = JSON.parse(readFileSync(DEPLOYMENT, "utf8"));

/** Real testnet protocol addresses (published by each project). */
const REAL = {
  xlm: "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
  /** Blend's testnet USDC, the asset Blend, DeFindex, Soroswap and Aquarius all list. */
  usdc: "CAQCFVLOBK5GIULPNZRGATJJMIZL5BSP7X5YJVMGCPTUEPFM4AVSRCJU",
  usdcIssuer: "GATALTGTWIOT6BUDBCZM3Q4OQ4BO2COLOAZ7IYSKPLC2PMSOPPGF5V56",
  blendPool: "CCEBVDYM32YNYCVNRXQKDFFPISJJCV557CDZEIRBEE4NCV4KHPQ44HGF",
  defindexVault: "CBMVK2JK6NTOT2O4HNQAIQFJY232BHKGLIMXDVQVHIIZKDACXDFZDWHN",
  aquariusPool: "CBXVVJ5BOTUVRPXUY54LXFILWC4ZTJWRP2GIVHBMOEAOPHSLGHBG3NNP",
};
const RWA: string = d.contracts.rwa; // the demo RWA token from 03-seed-demo.sh
const DAY = 86_400;

const secret = (k: string) => {
  const v = process.env[k];
  if (!v) throw new Error(`set ${k}`);
  return Keypair.fromSecret(v);
};
const [admin, maker, taker] = ["ADMIN_SECRET", "MAKER_SECRET", "TAKER_SECRET"].map(secret);
const sdk = new Octarine({ network: "testnet", contracts: d.contracts });
const as = (kp: Keypair) => sdk.as(keypairSigner(kp, NET.networkPassphrase));
const A = as(admin), M = as(maker), T = as(taker);
const server = new rpc.Server(NET.rpcUrl);

const report: { step: string; tx?: string; detail?: unknown }[] = [];
let failures = 0;

async function send<R>(label: string, tx: AssembledTransaction<R> | Promise<AssembledTransaction<R>>): Promise<R> {
  const { hash, result } = await submit(tx);
  report.push({ step: label, tx: hash });
  console.log(`  tx ${label}: ${hash}`);
  return result as R;
}
async function view<R>(tx: Promise<AssembledTransaction<R>>): Promise<R> {
  return (await tx).result;
}
function check(label: string, ok: boolean, detail?: unknown) {
  console.log(`  ${ok ? "✓" : "✗"} ${label}${detail === undefined ? "" : ` (${fmt(detail)})`}`);
  report.push({ step: `check: ${label}`, detail: { ok, detail: fmt(detail) } });
  if (!ok) failures++;
}
const fmt = (v: unknown) => JSON.stringify(v, (_, x) => (typeof x === "bigint" ? x.toString() : x));
const bal = (token: string, who: string) => view(sdk.token(token).balance({ id: who }));
const usd = (v: bigint) => `${formatUnits(v, 7)} USDC`;
const unwrap = <T>(r: T | { unwrap(): T }): T => (r && typeof r === "object" && "unwrap" in r ? (r as { unwrap(): T }).unwrap() : (r as T));

async function trustUsdc(kp: Keypair) {
  const acc = await server.getAccount(kp.publicKey());
  const tx = new TransactionBuilder(acc, { fee: "1000", networkPassphrase: NET.networkPassphrase })
    .addOperation(Operation.changeTrust({ asset: new Asset("USDC", REAL.usdcIssuer) }))
    .setTimeout(60)
    .build();
  tx.sign(kp);
  const r = await server.sendTransaction(tx);
  await server.pollTransaction(r.hash);
  report.push({ step: `trustline USDC ${kp.publicKey()}`, tx: r.hash });
}

async function step(name: string, fn: () => Promise<void>) {
  console.log(`\n== ${name}`);
  try {
    await fn();
  } catch (e) {
    failures++;
    console.log(`  ✗ ${name} threw: ${e instanceof Error ? e.message.split("\n")[0] : e}`);
    report.push({ step: name, detail: { error: String(e).slice(0, 500) } });
  }
}

// ---------------------------------------------------------------------------

const ctx: Record<string, string> = {};

await step("USDC trustlines", async () => {
  for (const kp of [admin, maker, taker]) await trustUsdc(kp);
  check("trustlines open", (await bal(REAL.usdc, taker.publicKey())) >= 0n);
});

await step("DEX aggregator over Soroswap (real pool): XLM → USDC", async () => {
  const dex = T.dexAggregator();
  const amount = 200_0000000n;
  const before = await bal(REAL.usdc, taker.publicKey());
  const route = [{ amount_in: amount, hops: [{ venue: d.contracts.soroswapAdapter, token_out: REAL.usdc, data: soroswapHopData() }] }];
  const r = await send(
    "dex.swap soroswap",
    dex.swap({ user: taker.publicKey(), desc: { token_in: REAL.xlm, token_out: REAL.usdc, amount_in: amount, min_out: 1n, to: taker.publicKey() }, route, partner: undefined }),
  );
  const got = (await bal(REAL.usdc, taker.publicKey())) - before;
  check("taker received USDC", got > 0n && got === unwrap(r).amount_out, usd(got));
});

await step("DEX aggregator over Aquarius (real pool): XLM → USDC", async () => {
  const dex = T.dexAggregator();
  const amount = 100_0000000n;
  const before = await bal(REAL.usdc, taker.publicKey());
  const route = [{ amount_in: amount, hops: [{ venue: d.contracts.aquariusAdapter, token_out: REAL.usdc, data: aquariusHopData(REAL.aquariusPool) }] }];
  await send(
    "dex.swap aquarius",
    dex.swap({ user: taker.publicKey(), desc: { token_in: REAL.xlm, token_out: REAL.usdc, amount_in: amount, min_out: 1n, to: taker.publicKey() }, route, partner: undefined }),
  );
  check("taker received USDC", (await bal(REAL.usdc, taker.publicKey())) > before);
});

await step("Router Dex leg: split across Soroswap and Aquarius", async () => {
  const amount = 200_0000000n;
  const ledger = (await server.getLatestLedger()).sequence;
  await send("approve router XLM", T.token(REAL.xlm).approve({ from: taker.publicKey(), spender: d.contracts.router, amount, expiration_ledger: ledger + 1000 }));
  const data = dexLegData({
    tag: "Splits",
    values: [[
      { amount_in: amount / 2n, hops: [{ venue: d.contracts.soroswapAdapter, token_out: REAL.usdc, data: soroswapHopData() }] },
      { amount_in: amount / 2n, hops: [{ venue: d.contracts.aquariusAdapter, token_out: REAL.usdc, data: aquariusHopData(REAL.aquariusPool) }] },
    ]],
  });
  const before = await bal(REAL.usdc, taker.publicKey());
  const r = await send(
    "router.fill dex",
    T.router().fill({
      taker: taker.publicKey(),
      taker_token: REAL.xlm,
      maker_token: REAL.usdc,
      route: [{ tag: "Dex", values: [{ aggregator: d.contracts.dexAggregator, taker_amount: amount, min_maker_amount: 1n, data }] }],
      min_out: 1n,
    }),
  );
  const got = (await bal(REAL.usdc, taker.publicKey())) - before;
  check("router delivered the split", got > 0n && got === unwrap(r).amount_out, usd(got));
  check("router holds nothing", (await bal(REAL.usdc, d.contracts.router)) === 0n && (await bal(REAL.xlm, d.contracts.router)) === 0n);
});

await step("Maker sources USDC for LP capital (Soroswap)", async () => {
  const amount = 1500_0000000n;
  await send(
    "dex.swap maker",
    M.dexAggregator().swap({
      user: maker.publicKey(),
      desc: { token_in: REAL.xlm, token_out: REAL.usdc, amount_in: amount, min_out: 1n, to: maker.publicKey() },
      route: [
        { amount_in: amount / 2n, hops: [{ venue: d.contracts.soroswapAdapter, token_out: REAL.usdc, data: soroswapHopData() }] },
        { amount_in: amount / 2n, hops: [{ venue: d.contracts.aquariusAdapter, token_out: REAL.usdc, data: aquariusHopData(REAL.aquariusPool) }] },
      ],
      partner: undefined,
    }),
  );
  const b = await bal(REAL.usdc, maker.publicKey());
  ctx.makerUsdc = b.toString();
  check("maker holds USDC", b > 0n, usd(b));
});

await step("Push oracle: keeper pushes Centrifuge JAAA NAV as the RWA price", async () => {
  const env = {
    ...process.env,
    PUSH_ORACLE: d.contracts.pushOracle,
    KEEPER_SECRET: admin.secret(),
    NAV_BASE: RWA,
    NAV_QUOTE: REAL.usdc,
    NAV_BASE_DECIMALS: "7",
    NAV_QUOTE_DECIMALS: "7",
  };
  const out = execFileSync("npx", ["tsx", "src/index.ts", "--once"], { cwd: new URL("../../keeper/", import.meta.url), env, encoding: "utf8" });
  console.log(out.trim().split("\n").map((l) => `  keeper: ${l}`).join("\n"));
  const hash = out.match(/pushed ([0-9a-f]{64})/)?.[1];
  if (hash) report.push({ step: "keeper push", tx: hash });
  const p = await view(sdk.pushOracle().get_price({ base: RWA, quote: REAL.usdc }));
  ctx.price = p.price.toString();
  check("oracle serves the NAV", p.price > 0n, { price: p.price, timestamp: p.timestamp });
});

await step("Facility stack: USDC vault on Blend + DeFindex, RWA vault, facility", async () => {
  const agg = A.facilityAggregator();
  const w = admin.publicKey();
  const vcfg = (asset: string, name: string) => ({
    asset, curator: w, manager: w, guardian: w, name, symbol: name, deposit_cap: 0n, curator_fee_bps: 0, timelock: 0n,
  });
  ctx.usdcVault = unwrap(await send("create_vault USDC", agg.create_vault({ config: vcfg(REAL.usdc, "oUSDC") })));
  ctx.rwaVault = unwrap(await send("create_vault RWA", agg.create_vault({ config: vcfg(RWA, "oRWA") })));
  ctx.facility = unwrap(
    await send(
      "create_facility",
      agg.create_facility({
        config: { base_vault: ctx.usdcVault, curator: w, manager: w, guardian: w, name: "Octarine LP", symbol: "oLP", curator_fee_bps: 1000, vest_seconds: BigInt(DAY), timelock: 0n },
      }),
    ),
  );
  const v = A.vault(ctx.usdcVault);
  for (const [label, wasm, venue] of [
    ["blend", d.wasmHashes.blendAdapter, REAL.blendPool],
    ["defindex", d.wasmHashes.defindexAdapter, REAL.defindexVault],
  ] as const) {
    await send(`vault.submit venue ${label}`, v.submit({ change: { tag: "Venue", values: [Buffer.from(wasm, "hex"), venue] } }));
    await send(`vault.accept venue ${label}`, v.accept({ kind: 2 }));
  }
  const venues = await view(sdk.vault(ctx.usdcVault).venues());
  ctx.blendAdapter = venues[0];
  ctx.defindexAdapter = venues[1];
  check("USDC vault has the Blend and DeFindex adapters", venues.length === 2, venues);

  const f = A.facility(ctx.facility);
  const terms: F.Terms = {
    accepted: true,
    bid_rate: 1000, // 10.00 bps/day
    ask_rate: 500,
    max_bps: 8000,
    oracle: d.contracts.pushOracle,
    max_price_age: BigInt(7 * DAY),
    redemption: w,
    schedule: { mode: { tag: "Rolling", values: undefined }, rolling_seconds: 30 * DAY, next_redemption_at: 0n, cycle_seconds: 0, max_bps_per_day: 2000 },
  };
  const [kind] = unwrap(await send("facility.submit Rwa", f.submit({ change: { tag: "Rwa", values: [RWA, ctx.rwaVault, terms] } })));
  check("SDK derives the listing's timelock kind", kind === rwaChangeKind(RWA), { kind });
  check("the listing shows as pending", (await view(sdk.facility(ctx.facility).pending({ kind }))) != null);
  await send("facility.accept Rwa", f.accept({ kind }));
  await send("set_active", agg.set_active({ facility: ctx.facility, active: true }));
  check("facility listed and routable", (await view(sdk.facilityAggregator().active())).includes(ctx.facility));
});

await step("LP deposits USDC; the manager allocates to Blend and DeFindex", async () => {
  const lp = BigInt(ctx.makerUsdc) / 2n;
  ctx.lp = lp.toString();
  await send("facility.deposit", M.facility(ctx.facility).deposit({ assets: lp, receiver: maker.publicKey(), from: maker.publicKey(), operator: maker.publicKey() }));
  const nav = await view(sdk.facility(ctx.facility).total_assets());
  check("NAV equals the deposit", nav === lp || nav === lp - 1n, usd(nav));
  const v = A.vault(ctx.usdcVault);
  const third = lp / 3n;
  await send("vault.allocate blend", v.allocate({ adapter: ctx.blendAdapter, amount: third }));
  await send("vault.allocate defindex", v.allocate({ adapter: ctx.defindexAdapter, amount: third }));
  const inBlend = await view(sdk.venue(ctx.blendAdapter).total_assets());
  const inDefindex = await view(sdk.venue(ctx.defindexAdapter).total_assets());
  check("Blend position (real pool)", inBlend > 0n && inBlend >= third - 2n, usd(inBlend));
  check("DeFindex position (real vault)", inDefindex > 0n && inDefindex >= third - 10n, usd(inDefindex));
  const navAfter = await view(sdk.facility(ctx.facility).total_assets());
  check("allocation does not move NAV beyond rounding", navAfter >= nav - 20n, usd(navAfter));
});

await step("Router Facility leg: sell RWA, paid partly out of Blend", async () => {
  const fac = sdk.facility(ctx.facility);
  const units = (BigInt(ctx.lp) * 6n) / 10n * 10n ** 18n / BigInt(ctx.price); // ~60% of NAV
  const q = await view(sdk.facilityAggregator().quote_all({ token_in: RWA, token_out: REAL.usdc, amount_in: units }));
  const quoted = q.find((x) => x.facility === ctx.facility)!.amount_out;
  const preview = sellQuote(units, BigInt(ctx.price), 1000, 30 * DAY);
  check("on-chain quote matches the SDK preview net of the 50 bps fee", quoted === preview - (preview * 50n) / 10_000n || quoted === preview - (preview * 50n + 9_999n) / 10_000n, { quoted, preview });
  const before = await bal(REAL.usdc, taker.publicKey());
  await send(
    "router.fill facility",
    T.router().fill({
      taker: taker.publicKey(),
      taker_token: RWA,
      maker_token: REAL.usdc,
      route: [{ tag: "Facility", values: [{ aggregator: d.contracts.facilityAggregator, taker_amount: units, min_maker_amount: quoted, data: facilityLegData({ facility: ctx.facility, payer: taker.publicKey() }) }] }],
      min_out: quoted,
    }),
  );
  check("taker paid the quote", (await bal(REAL.usdc, taker.publicKey())) - before === quoted, usd(quoted));
  check("RWA went straight into its vault", (await bal(RWA, ctx.rwaVault)) === units);
  const blendLeft = await view(sdk.venue(ctx.blendAdapter).total_assets());
  check("the payout drew on Blend once idle ran short", blendLeft < BigInt(ctx.lp) / 3n, usd(blendLeft));
  check("discount is vesting", (await view(fac.locked())) > 0n);
});

await step("Buy RWA back from the facility (facility aggregator swap)", async () => {
  const spend = 10_0000000n;
  const before = await bal(RWA, taker.publicKey());
  await send(
    "facility_aggregator.swap buy",
    T.facilityAggregator().swap({ payer: taker.publicKey(), recipient: taker.publicKey(), token_in: REAL.usdc, token_out: RWA, amount_in: spend, min_out: 1n, facility: ctx.facility }),
  );
  check("taker received RWA", (await bal(RWA, taker.publicKey())) > before);
});

await step("Redemption: book RWA to the issuer, settle with proceeds", async () => {
  const f = A.facility(ctx.facility);
  const held = await bal(RWA, ctx.rwaVault);
  const units = held / 2n;
  await send("facility.book", f.book({ rwa: RWA, amount: units }));
  const r = await view(sdk.facility(ctx.facility).redemption({ rwa: RWA }));
  check("receivable carried at the mark", !!r && r.amount === units, r);
  // The demo issuer redeems at the oracle NAV; the admin stands in and pays USDC.
  const proceeds = (r!.carried * 1001n) / 1000n;
  if ((await bal(REAL.usdc, admin.publicKey())) < proceeds) {
    await send("admin sources USDC", A.dexAggregator().swap({
      user: admin.publicKey(),
      desc: { token_in: REAL.xlm, token_out: REAL.usdc, amount_in: 1500_0000000n, min_out: 1n, to: admin.publicKey() },
      route: [{ amount_in: 1500_0000000n, hops: [{ venue: d.contracts.aquariusAdapter, token_out: REAL.usdc, data: aquariusHopData(REAL.aquariusPool) }] }],
      partner: undefined,
    }));
  }
  await send("facility.settle", f.settle({ rwa: RWA, proceeds }));
  check("redemption closed", (await view(sdk.facility(ctx.facility).redemption({ rwa: RWA }))) == null);
});

await step("Withdrawal queue: first come, first served", async () => {
  const agg = A.facilityAggregator();
  ctx.queue = unwrap(await send("create_queue", agg.create_queue({ config: { facility: ctx.facility, curator: admin.publicKey() } })));
  const f = A.facility(ctx.facility);
  const [kind] = unwrap(await send("facility.submit Queue", f.submit({ change: { tag: "Queue", values: [ctx.queue] } })));
  await send("facility.accept Queue", f.accept({ kind }));
  const shares = (await view(sdk.facility(ctx.facility).balance({ id: maker.publicKey() }))) / 4n;
  const id = unwrap(await send("queue.request", M.queue(ctx.queue).request({ owner: maker.publicKey(), receiver: maker.publicKey(), shares })));
  const before = await bal(REAL.usdc, maker.publicKey());
  await send("queue.process", T.queue(ctx.queue).process({ max: 5 }));
  const pos = await view(sdk.queue(ctx.queue).position({ id }));
  check("position served (anyone may process)", pos == null, { id });
  check("LP paid", (await bal(REAL.usdc, maker.publicKey())) > before);
});

await step("LP exits: the vault pulls back from DeFindex and Blend", async () => {
  const f = sdk.facility(ctx.facility);
  const max = await view(f.max_withdraw({ owner: maker.publicKey() }));
  const before = await bal(REAL.usdc, maker.publicKey());
  const take = max / 2n;
  await send("facility.withdraw", M.facility(ctx.facility).withdraw({ assets: take, receiver: maker.publicKey(), owner: maker.publicKey(), operator: maker.publicKey() }));
  check("LP received the withdrawal", (await bal(REAL.usdc, maker.publicKey())) - before === take, usd(take));
  const nav = await view(f.total_assets());
  const supply = await view(f.total_supply());
  check("NAV and supply consistent", nav > 0n && supply > 0n, { nav: usd(nav), supply });
});

writeFileSync(REPORT, JSON.stringify({ ranAt: new Date().toISOString(), contracts: ctx, failures, steps: report }, null, 2));
console.log(`\n${failures === 0 ? "ALL PASSED" : `${failures} FAILURE(S)`} — report: deployments/testnet-integration.json`);
process.exit(failures === 0 ? 0 : 1);
