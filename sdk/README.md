# @octarine/stellar-sdk

TypeScript SDK for Octarine on Stellar, for frontends and backends. It covers:

- typed clients for settlement, the router, facilities, vaults, the withdrawal
  queue, both aggregators, the push oracle, tokens and venue adapters;
- encoders for router legs;
- facility pricing previews;
- order signing;
- the auction API for market makers and takers.

```bash
npm install && npm run build
npm test                 # offline unit tests
npm run test:testnet     # end to end against real testnet protocols (needs funded keys, see below)
npm run gen              # regenerate src/contracts/* from ../target WASMs, then sync addresses
```

```ts
import { Octarine, keypairSigner, networks, submit } from "@octarine/stellar-sdk";
const sdk = new Octarine({ network: "testnet" }).as(keypairSigner(SECRET, networks.testnet.networkPassphrase));
const nav = (await sdk.facility(FACILITY).total_assets()).result;
await submit(sdk.facility(FACILITY).deposit({ assets: 100_0000000n, receiver: me, from: me, operator: me }));
```

Guides are in [`../docs/integration`](../docs/integration/README.md).

Examples:

- [`examples/market-maker.ts`](examples/market-maker.ts) is an RFQ bidding bot.
- [`examples/curator-ops.ts`](examples/curator-ops.ts) is a facility operations
  bot: it serves the queue, keeps an idle buffer, and watches price freshness.

`test/testnet.ts` needs `ADMIN_SECRET`, `MAKER_SECRET` and `TAKER_SECRET`. These
are funded testnet keys; the admin must be the deployment admin and the
push-oracle updater.

`src/contracts/*` and `src/deployments.ts` are generated; edit the contracts or
`deployments/*.json` instead.
