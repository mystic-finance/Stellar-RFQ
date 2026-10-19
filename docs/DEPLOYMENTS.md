# Deployments

The machine-readable source is [`deployments/testnet.json`](../deployments/testnet.json),
which the SDK reads through `sdk/scripts/sync.mjs`. Explorer:
`https://stellar.expert/explorer/testnet/contract/<id>`.

## Stellar testnet: Milestone 2 (current)

Deployed 2026-10-19 on protocol 29, from branch `milestone-2`. Admin, curator
and keeper: `GBH2FDJQSROZAOIWBCBAYPSPTXPV2TUQAIWF5IISC3DRWEV6Z24R3PN2`.

### Protocol

| Contract | Address | Notes |
|---|---|---|
| Settlement (`rfq`) | `CBGT6NPOUMWCT6K352PLCMVD3RAGXT54ZTGIKJY53RZL4QTM3SZSV2UV` | RFQ, fixed and Dutch orders |
| Router | `CCMJOSXZRPFAUDBAFQOUV6WPIMSFRIDDAGNPI3HNU77YDWOXKBRZHVCG` | sources: facility aggregator (Facility), DEX aggregator (Dex) |
| Facility aggregator | `CAM3BHUNJZYMYX5YZFWGT3NOEG55XO6IDAXAEESUBFZIMG5GVR5EEQLV` | factory, registry and router source; protocol fee 50 bps |
| DEX aggregator | `CCWJDXXAWV3V2S6XBEWK4O4CTPECPSMEXOMO3HFEUKBR75FR4ERLFFA4` | protocol fee 10 bps; partner fee ≤ 300 bps |
| Soroswap adapter | `CDBQF5E23TM4QX7TT2LWYI3EAJE5XVJTDUXOBV7J6ZFCG7FYPOSRVJRY` | over the Soroswap router |
| Aquarius adapter | `CAEFAJYCHANVOGGQIPMP6TYW2UO7F4D3TFS5KPZCDNUPYZZ5Z3QMFB6M` | pool named per hop |
| Push oracle | `CBGNVHHDHPCH7BPW4JGRTCJXVHJIHZWFDX3E5ZFJPOGNOUAPYIMAXHKY` | max deviation 200 bps per push |

### Code the factory deploys (wasm hashes)

| Code | Hash |
|---|---|
| vault | `29ab57d7cf02895b4bb0b3303d2146141b7b1caf86052c76671c8bc7d412b3cd` |
| facility | `834ee793d902120f90d3156adf8af963c347b13546b576a3a71b9045ade0935e` |
| withdrawal queue | `8be1a6e8c31fa2c2d53544ff6a634c59e4a89a24a2272c4993991e6dc100bd48` |
| Blend adapter (allowlisted) | `3db1866710154e0869bb3347920bd0773c3a48c11ca2d0ffb43bcb66cb883b48` |
| DeFindex adapter (allowlisted) | `973b72b18a03bb5e683871f8e30cbf4ef8ee282bb73cf12bea0ae710a33f1c13` |
| SEP-56 adapter (allowlisted) | `8703db09d8902324830f17f41069bc4e997e8db8b5611e3604312534fcc0fa9e` |
| mock venue (testnet only) | `4fe5deb90568a00360ef571f74acbc3a2991e03fb8326a0ae8120ff432a9f667` |

### Live facilities

Four facilities over USDC are routable from the router. Every RWA is priced by
the push oracle:

- **tCLO and ORWA** carry the live Centrifuge JAAA NAV, pushed by the keeper.
- **The other tokens** carry stand-in NAVs that the admin pushed.

Each USDC vault lends to real Blend v2 and/or DeFindex. Each facility has LP
capital and its first sales on-chain.

| Facility | Assets (bid / ask, bps/day · cap · redemption) | Lends idle cash to |
|---|---|---|
| **Octarine Treasury** `CCAY5JZOITKZOOFNHKG6K3OTZQZIJRJPBFOXEACVYM7UD6XEPXCJHNE3` | tTBILL (3.00 / 1.50 · 60% · 7d rolling), tMMF (1.50 / 0.50 · 60% · 1d rolling) | Blend v2 |
| **Octarine Credit** `CCAFPOL6UJ4Y23SYROA5TABUPG6NN274W5C4Y254VJBJQLBZSP6ASYCX` | tCLO (8.00 / 4.00 · 50% · 30d rolling), tCREDIT (12.00 / 6.00 · 40% · 30d cycle) | Blend v2, DeFindex |
| **Octarine Diversified** `CDXENRX2N7GWWEGNHNPIOVQMGAJUBHVSSLSWH742WRXWXLPXCJVV2HEY` | tTBILL (3.50 / 1.75 · 40%), tCLO (9.00 / 4.50 · 40%), tCREDIT (13.00 / 6.50 · 30%) | DeFindex |
| **Octarine LP** (integration run) `CBZV6CC6T4X3P4YDKPTAOS4U4ZLKTWCPNNV6UMRAMZX64RFK4Q3Q3UWE` | ORWA (10.00 / 5.00 · 80% · 30d rolling) | Blend v2, DeFindex |

Vaults, venue adapters and queues for each are under `.facilities` in
[`deployments/testnet.json`](../deployments/testnet.json).
`sdk/scripts/deploy-facilities.ts` recreates the set.

### Tokens

| Token | Address |
|---|---|
| USDC (Blend testnet, `USDC:GATALTGTWIOT6BUDBCZM3Q4OQ4BO2COLOAZ7IYSKPLC2PMSOPPGF5V56`, 7 dp) | `CAQCFVLOBK5GIULPNZRGATJJMIZL5BSP7X5YJVMGCPTUEPFM4AVSRCJU` |
| XLM (native SAC) | `CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC` |
| ORWA, demo RWA (7 dp) | `CBKP74OVRDQT2BA223L6JQTKM2UR55RASIUJ4TKS7JV24B6C4RB56NTO` |
| tTBILL (testnet T-bill fund, 7 dp) | `CDVPXGVQYKZ2JLYIG5RCTTOXUEOBPQAILZYUVNPBVBMC4DYZCCNFI5IR` |
| tMMF (testnet money-market fund, 7 dp) | `CD3LVKXRRCQ77ZJ4SVHKTEEU7KJN7GRDOSOGZSFJIENNHFVDT3G7QF6E` |
| tCLO (testnet AAA CLO, NAV from Centrifuge JAAA, 7 dp) | `CA4LDL4JSACLUPHAILPN4C2SBJIXRLJ2IPMFEVH7QIZ4JJPY3HYLHO45` |
| tCREDIT (testnet private credit, 7 dp) | `CB7FNDMUYGPCY5KXBQMIVYX2TL3OYKG4WMTILKWJVVYHIY2Y3LS7NK3Q` |
| OUSD, demo reference stable for settlement (7 dp) | `CBQ3EXUXCUKLUMNZRRF5SNYKPQPCTHXRSTA2WZIO7RLUBSZUV3SNMEIC` |

### External protocols used (published by each project)

| Protocol | Address |
|---|---|
| Soroswap router | `CCJUD55AG6W5HAI5LRVNKAE5WDP5XGZBUDS5WNTIVDU7O264UZZE7BRD` |
| Soroswap XLM/USDC pair | `CBR76WMT6J733CCVBP23M2EL5QGP5HXLPEFNFZGZ7IB6QHOJAHP7YM3V` |
| Aquarius router | `CBCFTQSPDBAIZ6R6PJQKSQWKNKWH2QIV3I4J72SHWBIK3ADRRAM5A6GD` |
| Aquarius XLM/USDC pool | `CBXVVJ5BOTUVRPXUY54LXFILWC4ZTJWRP2GIVHBMOEAOPHSLGHBG3NNP` |
| Blend v2 pool (`TestnetV2`) | `CCEBVDYM32YNYCVNRXQKDFFPISJJCV557CDZEIRBEE4NCV4KHPQ44HGF` |
| DeFindex USDC vault (Blend strategy) | `CBMVK2JK6NTOT2O4HNQAIQFJY232BHKGLIMXDVQVHIIZKDACXDFZDWHN` |

## Testnet verification

Two runs cover every contract:

- **`scripts/05-fill-demo.sh`** covers settlement and router: an RFQ order, a
  fixed order, a router-settled quoted bid, a Dutch listing, and refusal of a
  cancelled order.
- **`sdk/test/testnet.ts`** runs against the real protocols above. The latest
  run is from 2026-10-19 and passed every check (29 as of 2026-10-19); the transactions below are from an earlier passing run and remain on-chain. Its full log is
  [`deployments/testnet-integration.json`](../deployments/testnet-integration.json).

| Flow | Transaction |
|---|---|
| DEX aggregator over Soroswap, XLM → USDC | `d455335e28b929e79d336004c54167357aabfd69c5e711e9ed73e5f0798aa0bf` |
| DEX aggregator over Aquarius, XLM → USDC | `06c5ac7fec8f9f2c4eede1283c5885dfe5cfb9a4c32b833c11e6d31ee065bf7b` |
| Router Dex leg split across Soroswap + Aquarius | `0304692d534dd90e4628803bffdc20e140fadba2d1ff877060304b6f6ed2d281` |
| Keeper pushes the JAAA NAV (1.052397) to the push oracle | `5a6cb5831319a666399b1144bc14f71e118189bcc315e72766eb4d4601847290` |
| Factory creates the USDC vault / RWA vault / facility | `6cf8c620…144e` / `b3d03e13…1628` / `1ae0df45…b26f` |
| USDC vault enables Blend v2 / DeFindex (timelocked accept) | `78c349a6…2a26` / `5476876f…5ea8` |
| RWA listed through the timelock (submit / accept) | `af0afa5b…6768` / `b9630481…a515` |
| LP deposits USDC for oLP | `b28dcea94c46e9b817289531b67ab76ae099481714b18316f85271c3fa93ff2e` |
| Manager allocates to Blend v2 / DeFindex | `bd1c9765…ccfc` / `e5a4b0ad…5d3e` |
| **Router Facility leg: RWA sold, RWA lands in its vault, payout drawn partly from Blend** | `0366f5ff754e93642e456260e31a79916f1374d939918415603a56a97e824435` |
| Buy RWA back from the facility | `15d995a0d08120080f5ab472a100b8097441c16d8047242595d0e9acf4329bc5` |
| Book RWA for redemption / settle in full | `7582f8bb…77dc` / `29b4c380…24ec` |
| Queue request / served FIFO by a third party | `d45b425c…b0d3` / `88d74947…740d` |
| LP withdraws; vault pulls back from DeFindex and Blend | `b51fbd7a97365f3798db156fcba43a7adcdf55431a1bbbcf0b9e6fea4d59868f` |

To rerun the real-protocol run:

```bash
cd sdk && npm install
ADMIN_SECRET=$(stellar keys show rfq-admin) MAKER_SECRET=$(stellar keys show rfq-maker) \
TAKER_SECRET=$(stellar keys show rfq-taker) npm run test:testnet
```

Each run creates a fresh facility and activates it, so deactivate old ones with
`facility_aggregator.set_active(old, false)`.

## Stellar testnet: Milestone 1 (still live)

These are referenced by `MILESTONE_1.md`, and by the current mystic-backend and
Octarine-UI defaults:

- Settlement: `CDB75DJB7KK6V2CJPGT44CJRZYPP7BPXFHZTOPIYSGO2KGQC576UJYQM`
- Router: `CAVJVJ7QVIVJKR2DVBNBAPFGJLFL4PGW5QFFHVLWBGVWMDLDBPBT75P6`
- Price adapter: `CAGR33LOLRUMNYHGKE2P3Z55I67CMMCVTF2WNQITG526XAVPZGMMVNCC`
- mUSDC `CBHHOLNFBQSZ7TJ4TE3UFF43HJ4XSBLU6AXDIC5K73Z4TZUQSUCNA7PC`
- mRWA `CCVU3HJIL4EZ2C3RT5ICQ7LAZ3IIF3TRKYBW72HD7JINH2IUD2PDRDD7`
- mXLM `CBOOCLAKDO4J3EDXLVZJ3EKSCIKGQ5NFYOEYNXPQTHSECTJWB26KA7M3`

The Milestone 1 router predates Facility legs. To use facilities, point
the backend (`STELLAR_RFQ_CONTRACT_TESTNET`, `STELLAR_ROUTER_CONTRACT_TESTNET`)
and the UI (`VITE_STELLAR_RFQ`, router) at the Milestone 2 addresses above.

## Mainnet

Not deployed.
