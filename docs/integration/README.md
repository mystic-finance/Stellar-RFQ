# Integrating with Octarine on Stellar

Octarine sells tokenised real-world assets (RWAs) for stablecoins instantly,
at a discount priced per day until the asset redeems. Three kinds of
liquidity compete for every sale, and the **router** settles any mix of them in
one transaction:

```
seller ──▶ router ──┬─▶ rfq settlement        market makers' signed bids (auction API)
                    ├─▶ dex_aggregator        Soroswap, Aquarius, … (adapter routes or calldata)
                    └─▶ facility_aggregator ─▶ facility (oLP) ─┬─ USDC vault ─▶ Blend v2, DeFindex, SEP-56
                                                               ├─ RWA vault(s)
                                                               └─ withdrawal queue
                           push_oracle ◀── keeper (issuer NAV)
```

| You are | Start with |
|---|---|
| Building on the contracts from TypeScript | [sdk.md](sdk.md) |
| A wallet, app or solver selling for a user | [routing.md](routing.md) |
| A market maker bidding on RFQs (the LP SDK and bot) | [market-makers.md](market-makers.md) |
| A curator running a facility, or a depositor | [facilities.md](facilities.md) |
| An issuer or operator publishing NAV on-chain | [push-oracle.md](push-oracle.md) |

Addresses for every network are in [../DEPLOYMENTS.md](../DEPLOYMENTS.md).
Contract-level design is in [../FACILITY_V2_STELLAR.md](../FACILITY_V2_STELLAR.md)
and [../TECHNICAL_ARCHITECTURE.md](../TECHNICAL_ARCHITECTURE.md).

## Conventions across every contract

- **Amounts** are raw token units, as `i128`/`bigint`.
- **Rates** are hundredths of a basis point per day: `1000` is 10.00 bps/day.
  The auction API takes human bps/day.
- **Prices** are quote per base, in raw units ×1e18, with the source's timestamp.
- **Horizon** is the seconds to an asset's next redemption, from its schedule
  (`Rolling`: a fixed window; `Cyclical`: the next date on a grid).
- **No token passes through an intermediary.** Router, aggregators and facility
  hold nothing between transactions.
- **Governance changes are timelocked.** A curator submits, waits out the
  timelock, then accepts; a guardian can revoke. Pausing is immediate and never
  blocks exits.
