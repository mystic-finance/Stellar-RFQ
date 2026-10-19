/** A NAV reading: `value` units of the quote asset per one whole base unit, at `decimals` precision. */
export interface Nav {
  value: bigint;
  decimals: number;
  /** Unix seconds the issuer's NAV refers to. */
  timestamp: number;
}

export const ONE = 10n ** 18n;

/**
 * The oracle's convention: quote per base in *raw Stellar units*, 1e18-scaled.
 * e.g. NAV 1.052397 USDC per JAAA, USDC at 7 decimals and JAAA at 18 on
 * Stellar → 1.052397e7 raw USDC per 1e18 raw JAAA → price 10_523_970.
 */
export function toOraclePrice(nav: Nav, baseDecimals: number, quoteDecimals: number): bigint {
  return (nav.value * 10n ** BigInt(quoteDecimals) * ONE) / (10n ** BigInt(nav.decimals) * 10n ** BigInt(baseDecimals));
}

/** Move between two prices in basis points, rounded up. */
export function changeBps(from: bigint, to: bigint): bigint {
  if (from === 0n) return 10_000n;
  const diff = to > from ? to - from : from - to;
  return (diff * 10_000n + from - 1n) / from;
}

export interface Decision {
  push: boolean;
  reason: string;
}

/** Push on a big enough move or when the last reading is getting old; never past the oracle's bound. */
export function decide(
  last: { price: bigint; timestamp: number } | null,
  next: bigint,
  now: number,
  opts: { minChangeBps: number; heartbeatSeconds: number; maxDeviationBps: number },
): Decision {
  if (!last) return { push: true, reason: "first price" };
  const move = changeBps(last.price, next);
  if (opts.maxDeviationBps > 0 && move > BigInt(opts.maxDeviationBps)) {
    return { push: false, reason: `move of ${move} bps exceeds the oracle bound; needs the admin` };
  }
  if (move >= BigInt(opts.minChangeBps)) return { push: true, reason: `moved ${move} bps` };
  if (now - last.timestamp >= opts.heartbeatSeconds) return { push: true, reason: "heartbeat" };
  return { push: false, reason: `unchanged (${move} bps)` };
}
