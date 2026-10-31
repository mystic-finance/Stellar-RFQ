# The TypeScript SDK (`@octarine/stellar-sdk`)

`sdk/` is the TypeScript client for every Octarine contract on Stellar, plus
the auction API. The same package runs in a browser (wallet signing) and on a
server (keypair signing).

```bash
cd sdk && npm install && npm run build      # or depend on the folder: "file:../stellar-rfq/sdk"
```

Requires `@stellar/stellar-sdk` ≥ 17, a peer dependency. Testnet runs protocol
29, and earlier SDK versions cannot decode its transactions.

## What is in it

| Module | |
|---|---|
| `Octarine` | entry point: network, addresses, signer → typed clients for every contract |
| `rfq`, `router`, `facility`, `vault`, `queue`, `facilityAggregator`, `dexAggregator`, `pushOracle`, `token`, `venue` | generated clients and types, one per contract (`npm run gen` regenerates them from the WASMs) |
| `facilityLegData`, `dexLegData`, `aquariusHopData`, `soroswapHopData` | encoders for router-leg and hop `data` |
| `sellQuote`, `buyQuote`, `formatUnits`, `parseUnits`, `formatRate`, `rateApr` | facility pricing previews and unit helpers |
| `signOrderHash` | the SEP-53 signature settlement verifies |
| `submit` | sign, send, confirm, and wait for the RPC to catch up |
| `AuctionClient`, `placeBid`, `prepareMaker`, `executeTxns` | the auction API, for market makers and takers |

## Connecting

```ts
import { Octarine, keypairSigner, networks } from "@octarine/stellar-sdk";

// Read-only: views simulate without a signature.
const sdk = new Octarine({ network: "testnet" });

// Server: sign with a key.
const bot = sdk.as(keypairSigner(process.env.SECRET!, networks.testnet.networkPassphrase));

// Browser: sign with the wallet (Stellar Wallets Kit shown).
const app = sdk.as({
  publicKey: address,
  signTransaction: (xdr, opts) => StellarWalletsKit.signTransaction(xdr, { ...opts, address }),
});
```

Published addresses come from `deployments/<network>.json`; pass
`contracts: { … }` to override them. Facilities, vaults and queues are
discovered on-chain: `sdk.facilities()` lists the routable ones with their NAV.

## Calling contracts

Every method takes one object of named arguments, the same names as the Rust
entry point, and returns an `AssembledTransaction` that has already been
simulated.

```ts
// Views: read .result.
const nav = (await sdk.facility(F).total_assets()).result;          // bigint
const terms = (await sdk.facility(F).rwa({ rwa: RWA })).result;     // { vault, terms } | null

// Writes: sign and send.
const { hash, result } = await submit(app.facility(F).deposit({ assets: 100_0000000n, receiver: me, from: me, operator: me }));
```

Use `submit()` rather than `.signAndSend()` for writes. It throws when a
transaction fails on-chain, which `signAndSend` can leave silent. It also
tolerates functions that return nothing, and waits until the RPC's latest
ledger includes the write, so the next read sees it.

**Types.** `i128`/`u64` are `bigint`. `Option<T>` is `T | undefined` going in
and `T | null` coming out. Enums are `{ tag, values }`:
`{ tag: "Rwa", values: [rwa, vault, terms] }`. `BytesN` and `Bytes` are
`Buffer`.

**Errors.** A failed simulation throws with the contract error code in the
message (`Error(Contract, #7)`). Each client exports `Errors` mapping codes to
names, e.g. `facility.Errors[7]` is `{ message: "InsufficientLiquidity" }`. The
guides list what each one means.

## Units

Amounts are raw token units (USDC and most Stellar assets use 7 decimals). Rates
are **hundredths of a basis point per day**: `1000` is 10.00 bps/day, about
36.5% a year. Prices are quote per base in raw units ×1e18. The auction API
uses human bps/day (`2.5`). The SDK converts in `placeBid` and nowhere else.

## Where to go next

- Takers, wallets, solvers: [routing.md](routing.md)
- Market makers (the LP bot): [market-makers.md](market-makers.md)
- Curators and depositors: [facilities.md](facilities.md)
- Issuers and oracle operators: [push-oracle.md](push-oracle.md)
- Examples: `sdk/examples/market-maker.ts` and `sdk/examples/curator-ops.ts`.
  The real-protocol end-to-end run in `sdk/test/testnet.ts` uses every client.
