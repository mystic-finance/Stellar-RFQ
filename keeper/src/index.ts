import { load } from "./config.js";
import { decide, toOraclePrice } from "./convert.js";
import { erc4626Source } from "./sources/erc4626.js";
import { PushOracle } from "./stellar.js";

const cfg = load();
const source = erc4626Source({ name: "Centrifuge JAAA (Ethereum)", rpcUrl: cfg.evmRpcUrl, vault: cfg.vault });
const oracle = new PushOracle(cfg);
// Mirror the contract's bound so an out-of-range NAV is flagged, not sent.
const maxDeviationBps = await oracle.maxDeviationBps();

async function tick(): Promise<void> {
  const nav = await source.read();
  const price = toOraclePrice(nav, cfg.baseDecimals, cfg.quoteDecimals);
  const last = await oracle.lastPrice(cfg.base, cfg.quote);
  // Never claim a time ahead of the ledger: stamp a few seconds back.
  const stamp = Math.min(nav.timestamp, Math.floor(Date.now() / 1000) - 5);
  const d = decide(last, price, stamp, { ...cfg, maxDeviationBps });
  console.log(`${new Date().toISOString()} ${source.name}: NAV ${nav.value}/10^${nav.decimals} → price ${price}; ${d.reason}`);
  if (d.push) console.log(`  pushed ${await oracle.push(cfg.base, cfg.quote, price, stamp)}`);
}

const once = process.argv.includes("--once");
do {
  await tick().catch((e) => console.error(`tick failed: ${e instanceof Error ? e.message : e}`));
  if (!once) await new Promise((r) => setTimeout(r, cfg.intervalSeconds * 1000));
} while (!once);
