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

## Cross-chain settlement: Stellar testnet ↔ Ethereum Sepolia

Deployed 2026-10-23 against the real Circle CCTP V2 and LayerZero V2 contracts on both chains:
one settle contract per bridge, and off-chain picks which one to call by token and chains. See
[CROSSCHAIN_SETTLEMENT.md](CROSSCHAIN_SETTLEMENT.md). Testnet only: the governance delay is 0,
and the Sepolia RWA is priced by a mock oracle.

| Bridge | Stellar (`rfq-admin` admin) | Sepolia (`0xCfb24D0cd266F6a8C8F7D2833AB7AbAc831445b9` admin) | Chain ids | Tokens |
|---|---|---|---|---|
| CCTP: messages and USDC burns | `cctp_settle` `CCKV3CH6DXVTISCVJELPE6F3SFKCVVGMXKV2K4VWCJF7D5KG73KUOR4D` | `CctpSettle` `0x064b7fc61dd9563bffbdf943fc193bea71a5e3d8` | CCTP domains: Stellar 27, Sepolia 0 | USDC |
| LayerZero: messages and OFT payments | `lz_settle` `CBYDWHKIMK66CIVJ2OVURRI74XDHEH24E66FMXR7JPZAUZAHBLARBEGG` | `LzSettle` `0x82e25d0b7390906647849d778d9f9b6307704fea` | EIDs: Stellar 40600, Sepolia 40161 | none on testnet (no USDT0) |

**CCTP:** tip 0.50 USDC on Stellar and 0.10 USDC on Sepolia, paid to whoever `complete`s a
payment; no return fee. **LayerZero:** tip 0; Stellar → Sepolia options
`0x000301001101000000000000000000000000000f4240` (lzReceive, 1M gas) and return fee 1 XLM;
Sepolia → Stellar options `0x00030100110100000000000000000000000000030d40` and return fee 0.001
ETH; Stellar uses the LayerZero Labs DVN. With no OFT on testnet, `lz_settle` and `LzSettle` open
no route, so a fill through them is refused (`NoRoute`); the USDT0 path is covered by the
mainnet-fork tests. The deployed Stellar contracts predate CROSS_DELIVER (delivery 3) and the
per-token `bridge.ofts` config; no RWA OFT exists on testnet anyway, so a redeploy would refuse it
too.

### Supporting contracts

| Chain | Contract | Address | Notes |
|---|---|---|---|
| Stellar | RFQ (`rfq`) | `CB2WQHSH4JJZ6QYULRVR2HOYKMISP7HZT4OPFKZO3QYZFSI72JJON7QO` | reference Circle USDC; tTBILL pushed at 1.00 (30-day max age), 30-day rolling schedule |
| Stellar | Signature verifier | `CBVFZBSVYNT3ASKKLAI737AZ33XY5GIFQFCXSF7KDZK2UKFUVWXFXQBV` | |
| Stellar | RWA: tTBILL | `CDVPXGVQYKZ2JLYIG5RCTTOXUEOBPQAILZYUVNPBVBMC4DYZCCNFI5IR` | 7 decimals |
| Stellar | USDC (Circle) | `CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA` | `USDC:GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5`, 7 decimals |
| Stellar | CCTP TokenMessengerMinter / MessageTransmitter | `CDNG7HXAPBWICI2E3AUBP3YZWZELJLYSB6F5CC7WLDTLTHVM74SLRTHP` / `CBJ6MTCKKZG73PMDZCJMSFRD7DQEMI4FKDH7CGDSV4W6FHCRBCQAVVJY` | Circle |
| Stellar | LayerZero EndpointV2 / ULN302 | `CALTBA5S6GRJEHAXFP45LGGLKWWAF7HTZCPNUBUJF2HWWRRLQNV35AIV` / `CCMLPCAWCPIIMXOHJJKU3NZLOFTT2O6QTB2UUFPN6SEHLK35QRHVKKMB` | |
| Sepolia | RFQ (`OctarineRFQ`) | `0x8069f8e4e06afc9a291982e48aa96416dc69ffc7` | reference USDC |
| Sepolia | RWA: tACRED (test ERC-20, open `mint`) | `0x288495aa4de706d62ecab9141cd7d5f78a8d0a52` | 18 decimals |
| Sepolia | tACRED oracle (mock, open `setPrice`) | `0x4edf024cbd95776ff5160e444ae21a6dc66893d6` | 1.00 USDC, max age 365 days |
| Sepolia | USDC (Circle) | `0x1c7D4B196Cb0C7B01d743Fbc6116a902379C7238` | 6 decimals |
| Sepolia | CCTP TokenMessengerV2 / MessageTransmitterV2 | `0x8FE6B999Dc680CcFDD5Bf7EB0974218be2542DAA` / `0xE737e5cEBEEBa77EFE34D4aa090756590b1CE275` | Circle |
| Sepolia | LayerZero EndpointV2 | `0x6EDCE65403992e310A62460808c4b910D972f10f` | default libraries |

Deployed by [`scripts/10-deploy-crosschain.sh`](../scripts/10-deploy-crosschain.sh) (recorded
under `.crosschain` in `deployments/testnet.json`) and `octarine-v2/script/DeployCrossChainSettleSepolia.s.sol`.

### Live runs

| Run | Route | Steps |
|---|---|---|
| 2026-10-23, CCTP only | Stellar taker sells 10 tTBILL to a Sepolia maker for 9.925 USDC | fill on Stellar `9a31bc35bf2f506bf82f21c242a0cbea06892b1bcecf5cda9823e8c871d09e0d` (FILL as a CCTP message, attested in seconds) → maker's keeper relays it on Sepolia via `receiveMessage` `0xab9517752e76123a2a4eb6e219f8f4a5b741d3368ec24b618ae449b141728a76` (447k gas; maker's USDC burned home with PAID) → relayer `complete`s on Stellar `a540b314fe7da5dd1972f6863f3e25837a6069bfceaaa0bb37c08701af085f81`: taker +9.415075 USDC, fee 0.009925, tip 0.50, recipient +10 tTBILL |
| 2026-10-23, pre-split (retired `crosschain_settle` `CB7UMN…5MW` ↔ `LzSettle` `0x9DE7…8035`) | same trade, FILL over LayerZero, USDC over CCTP | fill `236d5e5bc21f93c4cd3c1eb8cbff1f201ae361f57a96492823f7f7b3d2b19ce0` → executor simulation reverted `LowGas()` at 200k gas; re-executed via `EndpointV2.lzReceive` `0x4fd17e9b6628596c13607d83ac2bf162cc5cb1aedee715688b2a1ce1bfde4349` → `complete` `fc59f9d6eeeb039de9b6ed293914748bf8f51dfda812bf52a9ce75404795cf7b` |

### Deployment gotchas

- **LayerZero on Stellar testnet:** the ULN's default DVN is archived and bound to an older ULN,
  so every send on the default pathway fails (error 1213). The deploy script sets the LayerZero
  Labs DVN `CB7EWGPRGD3R3CD2DIYIVXMOEFDINVBGBFFQLFMEMXFWJFJIMHA7ZLSA` for send and receive.
- **LayerZero options:** empty options are rejected by the real ULN on both chains. A FILL into
  Sepolia needs about 700k gas (`LzSettle` reverts `LowGas()` under 500k left, then pays), so
  Stellar's options for Sepolia carry 1M. A packet that failed for gas stays verified and anyone
  can execute it with `EndpointV2.lzReceive(origin, receiver, guid, message, "")`.
- **CCTP relaying:** a FILL relayed into Sepolia needs about 650k gas (447k used live).
- **Sepolia charges far more gas for contract creation than Foundry 1.8 simulates.** Broadcast
  with `--skip-simulation` so limits come from the node; if Foundry then fails to decode
  constructor arguments, `forge build --force` first.
- **Testnet USDC on Sepolia** comes from https://faucet.circle.com, or by swapping Sepolia ETH on
  Uniswap V3 (USDC/WETH 0.05% pool `0x3289680dD4d6C10bb19b899729cda5eEF58AEfF1`).
- **A `G…` taker or fee recipient needs a trustline to Circle USDC**, or the USDC is owed for `withdraw`.

### Testing it by hand: the CCTP route, a Stellar taker and a Sepolia maker

1. **Maker (Sepolia)** approves `CctpSettle` for USDC:
   `cast send 0x1c7D4B196Cb0C7B01d743Fbc6116a902379C7238 "approve(address,uint256)" 0x064b7fc61dd9563bffbdf943fc193bea71a5e3d8 <amount> --private-key $EVM_PRIVATE_KEY -r $SEPOLIA_RPC_URL`.
2. **Request (Stellar):** `maker_token` Stellar USDC, `taker_token` tTBILL, `sender`
   `cctp_settle`, `taker` the taker, `order_type` `Rfq`; its id is `rfq.hash_request(request)`.
3. **Bid:** `maker` and `stable` are the maker's address and Sepolia USDC as 32-byte words;
   `max_amount` is in 6-decimal wire units; `recipient` is the Stellar account (raw 32 bytes of the
   `G…`/`C…` key) that receives the tTBILL. The maker signs the EIP-712 digest
   `CctpSettle.hashBid(requestId, 27, bid)` with `cast wallet sign --no-hash`.
4. **Fill (Stellar):** the taker calls `cctp_settle.fill(request, 0, bid, signature)`. The tTBILL is
   escrowed and the FILL leaves as a CCTP message.
5. **Relay the FILL (maker's keeper):** `GET https://iris-api-sandbox.circle.com/v2/messages/27?transactionHash=<stellar tx>`,
   then `cast send 0xE737e5cEBEEBa77EFE34D4aa090756590b1CE275 "receiveMessage(bytes,bytes)" <message> <attestation> --gas-limit 1000000`.
   `CctpSettle` pulls the maker's USDC and burns it home with PAID (or sends FAIL).
6. **Complete (anyone, for the tip):** once Sepolia finalises (about 15 minutes),
   `GET https://iris-api-sandbox.circle.com/v2/messages/0?transactionHash=<sepolia tx>`, then
   `cctp_settle.complete(caller, message, attestation)`. The taker gets the USDC less fee and tip,
   the caller the tip, and the recipient the tTBILL.

A FAIL coming back (bad signature, expired bid, no allowance) is relayed by the taker on Stellar by
calling the transmitter's `receive_message(caller, message, attestation)` directly; it returns the
tTBILL. In the other direction (Sepolia taker, Stellar maker), a FILL arriving on Stellar is
relayed the same way and then run with `cctp_settle.execute(message)`, since Soroban forbids the
reply re-entering the transmitter.

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
