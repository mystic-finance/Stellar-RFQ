/**
 * Example market-maker bot for Octarine RFQs on Stellar.
 *
 * It listens for new requests (WebSocket, with a polling fallback) and prices
 * each one from a per-token rate table. It skips requests it cannot fund, then
 * quotes, verifies, signs and submits the bid. Won bids settle on-chain when
 * the taker fills; the bot only needs inventory and a standing allowance to
 * the settlement contract.
 *
 *   AUCTION_URL=https://curator-api.mysticfinance.xyz AUCTION_API_KEY=... \
 *   MAKER_SECRET=S... RATES='{"C...RWA":2.5}' npx tsx examples/market-maker.ts
 *
 * RATES maps the token you buy (the taker's `redeemAsset`) to your bid in
 * bps/day. A lower rate is a better price for the seller, so it is more likely
 * to win.
 */
import { Keypair } from "@stellar/stellar-sdk";
import { AuctionClient, Octarine, keypairSigner, networks, placeBid, prepareMaker, STELLAR_CHAIN_IDS, formatUnits, type AuctionRequest } from "../src/index.js";

const env = (k: string, d?: string) => {
  const v = process.env[k] ?? d;
  if (v === undefined) throw new Error(`set ${k}`);
  return v;
};
const network = env("NETWORK", "testnet") as "testnet" | "mainnet";
const maker = Keypair.fromSecret(env("MAKER_SECRET"));
const rates: Record<string, number> = JSON.parse(env("RATES", "{}"));
const defaultRate = Number(env("DEFAULT_BPS_PER_DAY", "0")); // 0 = only bid on tokens in RATES
const maxNotional = BigInt(env("MAX_NOTIONAL", "0")); // largest payout per request, raw units; 0 = no cap
const pollMs = Number(env("POLL_SECONDS", "30")) * 1000;

const sdk = new Octarine({ network }).as(keypairSigner(maker, networks[network].networkPassphrase));
const auction = new AuctionClient({ baseUrl: env("AUCTION_URL"), apiKey: env("AUCTION_API_KEY"), chainId: STELLAR_CHAIN_IDS[network] });
const seen = new Set<string>();

/** Your pricing. Return a rate in bps/day, or undefined to pass. */
function price(r: AuctionRequest): number | undefined {
  const rate = rates[r.redeemAsset] ?? (defaultRate || undefined);
  if (rate === undefined) return undefined;
  const cap = r.metadata.maxBpsPerDay;
  if (cap !== undefined && rate > cap) return undefined; // the seller will not accept that discount
  return rate;
}

/** Can we pay what we would owe if we win? */
async function canFund(r: AuctionRequest): Promise<boolean> {
  const owe = BigInt(r.metadata.indicativeMakerAmount ?? "0");
  if (maxNotional > 0n && owe > maxNotional) return false;
  const token = sdk.token(r.redemptionAsset);
  const [bal, allowance] = await Promise.all([
    token.balance({ id: maker.publicKey() }),
    token.allowance({ from: maker.publicKey(), spender: sdk.contracts.rfq! }),
  ]);
  if (bal.result < owe) return log(r, `skip: balance ${bal.result} < ${owe}`), false;
  if (allowance.result < owe) {
    log(r, "topping up the settlement allowance");
    await prepareMaker(sdk, { makerToken: r.redemptionAsset, amount: owe * 10n });
  }
  return true;
}

async function consider(r: AuctionRequest) {
  if (seen.has(r.requestId) || r.chainId !== auction.chainId) return;
  seen.add(r.requestId);
  const rate = price(r);
  if (rate === undefined) return log(r, "pass: no rate for this token");
  if (!(await canFund(r))) return;
  try {
    const res = await placeBid(auction, sdk, maker, { requestId: r.requestId, maker: maker.publicKey(), makerBpsPerDay: rate });
    const d = r.metadata.redemptionAssetData?.decimals ?? 7;
    log(r, `bid ${rate} bps/day → ~${formatUnits(BigInt(res.data?.makerAmount ?? 0), d)} (${res.data?.bidId})`);
  } catch (e) {
    log(r, `bid failed: ${e instanceof Error ? e.message : e}`);
  }
}

function log(r: AuctionRequest, msg: string) {
  console.log(`${new Date().toISOString()} ${r.requestId.slice(0, 10)} ${msg}`);
}

async function poll() {
  const page = await auction.requests({ marketMaker: maker.publicKey(), excludeBidOn: true, limit: 50 });
  for (const r of page.data) await consider(r);
}

console.log(`market maker ${maker.publicKey()} on ${network}; rates ${JSON.stringify(rates)}`);
await auction.subscribe({
  onRequest: (r) => void consider(r),
  onBidStatus: (e) => console.log(`${new Date().toISOString()} bid ${e.bidId} ${e.oldStatus} → ${e.newStatus}`),
}).catch((e) => console.warn(`websocket unavailable (${e.message}); polling only`));
for (;;) {
  await poll().catch((e) => console.warn(`poll failed: ${e.message}`));
  if (process.argv.includes("--once")) process.exit(0);
  await new Promise((r) => setTimeout(r, pollMs));
}
