# Pricing RWAs: the push oracle and the NAV keeper

Most tokenised RWAs have no market feed. Their price is the NAV the issuer
publishes, on its API or on another chain. The **push oracle** carries that NAV
onto Stellar, and the **keeper** (`keeper/`) is the bot that does it.

## The contract

`push_oracle` implements the same `get_price(base, quote) -> {price, timestamp}`
view as every Octarine oracle, so a curator names it as an RWA's `oracle` like
any other.

| Function | Who | |
|---|---|---|
| `push(caller, base, quote, price, timestamp)` | updater or admin | `price` is quote per base in raw units ×1e18; `timestamp` is the **source's** time (the NAV date), not the push time |
| `get_price(base, quote)` | anyone | the last push; reverts `NoPrice` (5) before the first |
| `set_updater(updater, allowed)` | admin | grant or revoke a keeper key |
| `set_max_deviation(bps)` | admin | largest move an updater may push in one step (0 = none) |
| `set_admin(admin)` | admin | |

**Rules for a push:**

- The timestamp must be newer than the last push, and not in the future.
- The price must be positive.
- An updater's move must stay within `max_deviation_bps` (`DeviationTooLarge`,
  4). The admin is exempt, so it can correct a bad price or follow a real jump
  after checking it.

**Freshness is the consumer's decision.** The oracle serves the last price with
its timestamp. A facility rejects it once it is older than that RWA's
`max_price_age`, and stops quoting the RWA until a fresh push arrives. Set
`max_price_age` to the issuer's NAV cadence plus a margin: for a daily NAV, 2 days.

## The price convention

`price = NAV × 10^quote_decimals × 1e18 / (10^nav_decimals × 10^base_decimals)`.
For example, a NAV of 1.052397 USDC per share, for a 7-decimal RWA token priced in
7-decimal USDC, is `1_052_397 × 10^7 × 1e18 / (10^6 × 10^7)` = `1.052397e18`.

## Running the keeper

```bash
cd keeper && npm install
export PUSH_ORACLE=C...            # the oracle
export KEEPER_SECRET=S...          # an account the admin made an updater
export NAV_BASE=C...  NAV_QUOTE=C... # the RWA token and the base asset on Stellar
export NAV_BASE_DECIMALS=7 NAV_QUOTE_DECIMALS=7
npm run once     # read, decide, push once
npm start        # loop every INTERVAL_SECONDS (default 300)
```

| Env | Default | |
|---|---|---|
| `STELLAR_RPC_URL`, `STELLAR_PASSPHRASE` | testnet | |
| `EVM_RPC_URL` | public Ethereum RPC | where the NAV source lives |
| `NAV_VAULT` | Centrifuge JAAA/USDC `0x4880…780b` | any ERC-4626 vault: NAV = `convertToAssets(10^decimals)` |
| `MIN_CHANGE_BPS` | 5 | skip a push that moves less than this… |
| `HEARTBEAT_SECONDS` | 21600 | …unless the last push is older than this |

Each tick the keeper:

1. reads the NAV from the source;
2. converts it to the oracle convention;
3. reads the contract's last price and `max_deviation_bps`;
4. pushes the price if it has moved at least `MIN_CHANGE_BPS`, or if the last
   push is older than `HEARTBEAT_SECONDS`.

The push is stamped `min(source time, now − 5 s)`. A move larger than the
contract's bound is logged and left for the admin to push by hand.

### Another issuer

A source is anything that implements `NavSource` (`keeper/src/sources/types.ts`):

```ts
export interface NavSource {
  name: string;
  read(): Promise<{ value: bigint; decimals: number; timestamp: number }>;
}
```

`sources/erc4626.ts` is the on-chain example. For an issuer that publishes NAV
over HTTP, implement `read()` as a fetch of its NAV endpoint, then swap it in
`src/index.ts`.

### Operating it

- **Keys.** The keeper key needs XLM for fees and nothing else. The admin key is
  the only one that can exceed the deviation bound; keep it offline.
- **Monitoring.** Alert when `now − get_price().timestamp` approaches the
  facilities' `max_price_age`. A facility that stops quoting is the symptom.
- **One oracle, many pairs.** Prices are keyed by `(base, quote)`, so one oracle
  serves every RWA. Run one keeper per pair, or one keeper with several sources.
