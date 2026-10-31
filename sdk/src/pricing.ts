/**
 * Off-chain previews of the facility's pricing, for UIs and bots. The contract
 * is the source of truth (`facility.quote`); these match it to the unit.
 */
export const ONE = 10n ** 18n;
export const BPS = 10_000n;
/** Rates are hundredths of a basis point per day. */
export const RATE_SCALE = 100n;
export const DENOM = BPS * RATE_SCALE * 86_400n;

/** `rate` (hundredths of a bp/day) over `horizonSeconds`, as a fraction of DENOM. */
export const discount = (rate: number, horizonSeconds: number): bigint => BigInt(rate) * BigInt(Math.max(1, horizonSeconds));

/** Base paid for `units` of RWA at oracle `price` (1e18), at the bid. */
export function sellQuote(units: bigint, price: bigint, bidRate: number, horizonSeconds: number): bigint {
  const mark = (units * price) / ONE;
  const d = discount(bidRate, horizonSeconds);
  return d >= DENOM ? 0n : (mark * (DENOM - d)) / DENOM;
}

/** RWA units bought with `value` of base at oracle `price` (1e18), at the ask. */
export function buyQuote(value: bigint, price: bigint, askRate: number, horizonSeconds: number): bigint {
  const factor = DENOM - discount(askRate, horizonSeconds);
  if (factor <= 0n) return 0n;
  return (((value * ONE) / price) * DENOM) / factor;
}

/** Display a rate: 1000 → "10.00 bps/day". */
export const formatRate = (rate: number): string => `${(rate / 100).toFixed(2)} bps/day`;

/** Annualised yield of a bid: rate × 365 days, as a percentage. */
export const rateApr = (rate: number): number => (rate / 100 / 10_000) * 365 * 100;

/** Raw units → decimal string (`1234567, 7` → "0.1234567"). */
export function formatUnits(v: bigint, decimals: number): string {
  const neg = v < 0n;
  const s = (neg ? -v : v).toString().padStart(decimals + 1, "0");
  const out = decimals ? `${s.slice(0, -decimals)}.${s.slice(-decimals)}`.replace(/\.?0+$/, "") : s;
  return (neg ? "-" : "") + out;
}

/** Decimal string → raw units ("0.5", 7 → 5000000n). */
export function parseUnits(v: string, decimals: number): bigint {
  const [i, f = ""] = v.trim().split(".");
  if (f.length > decimals) throw new Error(`${v} has more than ${decimals} decimals`);
  return BigInt(i || "0") * 10n ** BigInt(decimals) + BigInt(f.padEnd(decimals, "0") || "0");
}

/** Seconds to the next redemption under a schedule, as the contracts compute it. */
export function horizonSeconds(
  s: { mode: { tag: "Rolling" | "Cyclical" }; rolling_seconds: number; next_redemption_at: bigint; cycle_seconds: number },
  now = Math.floor(Date.now() / 1000),
): number {
  if (s.mode.tag === "Rolling") return s.rolling_seconds;
  let anchor = Number(s.next_redemption_at);
  if (now >= anchor && s.cycle_seconds > 0) anchor += (Math.floor((now - anchor) / s.cycle_seconds) + 1) * s.cycle_seconds;
  return Math.max(1, anchor - now);
}
