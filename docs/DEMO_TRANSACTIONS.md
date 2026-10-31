# Demo transactions

Every flow of the Octarine protocol on Stellar, run live on **Stellar testnet** (and **Ethereum
Sepolia** for the cross-chain flows) on **30 October 2026**. Each row links to the transaction on
[stellar.expert](https://stellar.expert/explorer/testnet) or
[Sepolia Etherscan](https://sepolia.etherscan.io). Contract addresses are in
[DEPLOYMENTS.md](DEPLOYMENTS.md).

Cross-chain flows use the real **Circle CCTP V2** contracts on both chains, with Circle's sandbox
attestation service. Facility and DEX flows use the real **Blend v2**, **DeFindex**, **Soroswap** and
**Aquarius** deployments on Stellar testnet.

## What is covered

| Area | Scenarios run | Section |
|---|---|---|
| RFQ settlement | RFQ order, fixed-price order, router-settled quote, Dutch auction listing, cancelled order refused | [1](#1-rfq-settlement-on-stellar) |
| DEX aggregator and router | Soroswap, Aquarius, one router leg split across both | [2](#2-dex-aggregator-and-router) |
| RWA pricing | Keeper pushes Centrifuge JAAA's NAV from Ethereum to the push oracle | [3](#3-push-oracle-keeper) |
| Liquidity facility | Create vaults and facility, timelocked listings, LP deposit, idle cash to Blend and DeFindex, RWA sold through the router, RWA bought back, redemption booked and settled, withdrawal queue, LP exit, curator pause | [4](#4-liquidity-facility-lifecycle) |
| Redemption vault | AUTO batch (verified issuer calls), MANUAL batch (operator, NAV floor), cancel, claim-token resale, partial return in kind | [5](#5-redemption-vault) |
| Cross-chain RFQ | DELIVER both directions through the backend API; REDEEM_CT; REDEEM_PUSH; maker reneges (FAIL, RWA returned); CROSS_DELIVER refused on CCTP | [6](#6-cross-chain-rfq-stellar--sepolia-over-cctp) |

## Accounts

| Account | Address | Role in these runs |
|---|---|---|
| `rfq-admin` | [`GBH2FD…3PN2`](https://stellar.expert/explorer/testnet/account/GBH2FDJQSROZAOIWBCBAYPSPTXPV2TUQAIWF5IISC3DRWEV6Z24R3PN2) | Protocol admin, facility curator and keeper; redemption-vault admin, settler and operator; plays the RWA issuer's redemption desk |
| `rfq-maker` | [`GDKL2G…7HVV`](https://stellar.expert/explorer/testnet/account/GDKL2GUQ44W5TYZVI7TN2RPWEIQJ7C6XZDISERV5LPHYFBJSDCDR7HVV) | Market maker in the Stellar RFQ flows; facility LP; redemption-vault holder; recipient of RWA and claim tokens in cross-chain fills; the Stellar taker in run 6.1 |
| `rfq-taker` | [`GAUXHH…FFAA`](https://stellar.expert/explorer/testnet/account/GAUXHH6CISOOSKG4U6YLTXIVWLOM27LKFGU27CZIV4UF7SRSVM24FFAA) | Seller of RWA (taker) in the Stellar RFQ flows and cross-chain fills 6.3, 6.4 and 6.6; the Stellar maker in run 6.2 |
| Sepolia wallet | [`0xCfb24D…45b9`](https://sepolia.etherscan.io/address/0xCfb24D0cd266F6a8C8F7D2833AB7AbAc831445b9) | Maker bidding from Sepolia (6.1, 6.3, 6.4, 6.6); taker on Sepolia (6.2); also the Sepolia fee recipient |

## 1. RFQ settlement on Stellar

`scripts/05-fill-demo.sh` against the settlement contract (`rfq`) and router, with demo tokens ORWA (RWA)
and OUSD (stable). Real signatures; every balance change is asserted.

| # | What happens | Chain | Transaction |
|---|---|---|---|
| 1 | Admin pushes the ORWA price (1.0 OUSD) used by the RFQ rate model | Stellar | [`aa815a80…`](https://stellar.expert/explorer/testnet/tx/aa815a807bbf782dd65bd65b583b49daaa61f9726a9bff4f909e784a45a8a7df) |
| 2 | Taker approves the settlement contract for ORWA | Stellar | [`dbd198a4…`](https://stellar.expert/explorer/testnet/tx/dbd198a42477d4f669f2fd57d891dade1918a7f6e265f5821a4c281bd084304f) |
| 3 | Maker approves the settlement contract for OUSD | Stellar | [`e9680866…`](https://stellar.expert/explorer/testnet/tx/e968086669c63126b9da0afb693c3ab840f1249ba1bb8289d4a335754efbaaee) |
| 4 | **RFQ order:** taker sells 1 ORWA at the maker's 10 bps/day over a 30-day horizon, receiving 0.97 OUSD (3% discount) | Stellar | [`caa1b196…`](https://stellar.expert/explorer/testnet/tx/caa1b19659196988eb7c57203bd452e2540211238af6d5ce678f1b935beb3449) |
| 5 | **Fixed order:** the maker names the amount (no oracle, no schedule); the protocol fee goes to the fee recipient | Stellar | [`c16bbe95…`](https://stellar.expert/explorer/testnet/tx/c16bbe95d46faa7c698f72a535be6433e7ecf974137df21a34e4cc491e577a61) |
| 6 | **Router:** a bid quoted to a named taker is settled by the router in one transaction; the router keeps nothing | Stellar | [`952d0f0a…`](https://stellar.expert/explorer/testnet/tx/952d0f0a748ba6226664a60619484fbf0bbc330e9648172825049134240560b6) |
| 7 | **Dutch listing:** the seller escrows 1 ORWA with a decaying ask | Stellar | [`1bf27933…`](https://stellar.expert/explorer/testnet/tx/1bf27933ffa9ef20ad1eb4e8d5792f87535d98ffc211149ef264678eac2cb03e) |
| 8 | **Dutch fill:** the maker buys from escrow at the current ask (0.9999951 OUSD) | Stellar | [`a57b39c5…`](https://stellar.expert/explorer/testnet/tx/a57b39c5af38c44c6b03fa580a166af182179c66f74feefc31874fe96f0a5a8c) |
| 9 | **Salt cancellation:** the maker cancels a signed bid's salt; the taker's attempt to fill it afterwards is refused (that attempt is simulated, so it has no transaction) | Stellar | [`597ed9b2…`](https://stellar.expert/explorer/testnet/tx/597ed9b268834099a63a50d9ab67e8a7b164eed0a3e161f5e09a5160062884f7) |

## 2. DEX aggregator and router

From `sdk/test/testnet.ts`, against the real Soroswap and Aquarius pools.

| # | What happens | Chain | Transaction |
|---|---|---|---|
| 1 | DEX aggregator swaps XLM → USDC through the **Soroswap** adapter | Stellar | [`e8b53e55…`](https://stellar.expert/explorer/testnet/tx/e8b53e556806223c4adf5e71967023b1eea9a737f8c4970e3c1cebed69e5ff82) |
| 2 | DEX aggregator swaps XLM → USDC through the **Aquarius** adapter | Stellar | [`4c8f21e1…`](https://stellar.expert/explorer/testnet/tx/4c8f21e1254005f6e727a2ecb31dcf6a1fb9dbdcc24b24071490c5f7c47f7b72) |
| 3 | Taker approves the router for XLM | Stellar | [`38aec198…`](https://stellar.expert/explorer/testnet/tx/38aec1987fb6783f67a739b886bf95d35f0c36ea51d82f10ee9d396ed602d6d0) |
| 4 | **Router Dex leg split across Soroswap and Aquarius** in one transaction (16.53 USDC delivered; router holds nothing) | Stellar | [`843935ce…`](https://stellar.expert/explorer/testnet/tx/843935ce4df6b3e093251f7a423942ee0ce1c8875b24bc1e2a4b868782d5f99e) |
| 5 | Maker sources USDC for LP capital through the aggregator | Stellar | [`199b7c84…`](https://stellar.expert/explorer/testnet/tx/199b7c845be913ce3c2501834fa269781e84631028c6661b8f379745d292eb4a) |

## 3. Push oracle keeper

The TypeScript keeper reads Centrifuge JAAA's NAV from its ERC-4626 vault on Ethereum mainnet
(1.054426) and pushes it to the push oracle [`CBGNVH…XHKY`](https://stellar.expert/explorer/testnet/contract/CBGNVHHDHPCH7BPW4JGRTCJXVHJIHZWFDX3E5ZFJPOGNOUAPYIMAXHKY), which prices the
facility's RWA. Each push is bounded per step (it moved 20 bps here).

| # | What happens | Chain | Transaction |
|---|---|---|---|
| 1 | Keeper pushes the JAAA NAV as the RWA price | Stellar | [`08e02190…`](https://stellar.expert/explorer/testnet/tx/08e0219017ba6ed6b5d27f99c300e700c1d7aa9c36abd7ba54da51494121c744) |

## 4. Liquidity facility lifecycle

A fresh facility [`CB7HJZ…5VWY`](https://stellar.expert/explorer/testnet/contract/CB7HJZAUK2P2KU42WT2RCKNLXZHXERSRS3PYQRCWMNPK3DKHIRIZ5VWY) over USDC vault [`CDHCYS…BWBC`](https://stellar.expert/explorer/testnet/contract/CDHCYSKTUC5E3HVWMERRSBTHCQO2C4IFNLDN4WG2P4FL253RKLDYBWBC) and RWA vault [`CCXHZW…ECXD`](https://stellar.expert/explorer/testnet/contract/CCXHZWA26LOUYFI6NNDQKTG7RMWNCM5XHE4XDW5W54K4ARTD6QT4ECXD),
created by the facility aggregator (factory). The USDC vault lends idle cash to the real Blend v2 pool
and DeFindex vault through allowlisted adapters.

| # | What happens | Chain | Transaction |
|---|---|---|---|
| 1 | Factory creates the USDC vault | Stellar | [`8625d750…`](https://stellar.expert/explorer/testnet/tx/8625d75018307a7f534ff63c801a3057550c4fcead9f7dba03128661e7ed960a) |
| 2 | Factory creates the RWA vault (the RWA never sits in the facility or router) | Stellar | [`edfe9b33…`](https://stellar.expert/explorer/testnet/tx/edfe9b339c0598e5d9e04d71500cba79af8416b5f28d56f4c6dc30936df7fb87) |
| 3 | Factory creates the facility | Stellar | [`a79e0dc3…`](https://stellar.expert/explorer/testnet/tx/a79e0dc3a4fcdd1523073b461b6d5f72d6679e34b7eef4d49921a1c54e411933) |
| 4 | Timelock: queue the Blend v2 venue on the USDC vault | Stellar | [`c527ecea…`](https://stellar.expert/explorer/testnet/tx/c527ecea12b4e53e2db4af3ca87e56b136f7ea56f91de2656cccda127c93f933) |
| 5 | Timelock: accept the Blend v2 venue | Stellar | [`9b215a73…`](https://stellar.expert/explorer/testnet/tx/9b215a73ac628dbae65a5f6a5427a4ab29f4939087f33aacd36b713194739e85) |
| 6 | Timelock: queue the DeFindex venue | Stellar | [`791322d4…`](https://stellar.expert/explorer/testnet/tx/791322d47a1937b703b9c575981b8104b4d14f77c80f72e31fe4564023054ad5) |
| 7 | Timelock: accept the DeFindex venue | Stellar | [`a692bc31…`](https://stellar.expert/explorer/testnet/tx/a692bc3105fc09bacf1ec28b64e98c34df85776cb1819ad44e84af23382b167d) |
| 8 | Timelock: queue the RWA listing (bid/ask bps/day, cap, redemption schedule, price feed) | Stellar | [`4454c750…`](https://stellar.expert/explorer/testnet/tx/4454c75030b40bcbb883d3c13bd1a1b6fd947d6c6f474cbd77e639fe1df6fb17) |
| 9 | Timelock: accept the RWA listing | Stellar | [`671ab8ef…`](https://stellar.expert/explorer/testnet/tx/671ab8eff4e9fdb92f070f2bce219af9fcf66d4e8ddfb0d2c0935f11bd474be0) |
| 10 | The aggregator activates the facility as a router source | Stellar | [`37a372b6…`](https://stellar.expert/explorer/testnet/tx/37a372b6d91fc02cd643531d14f509016d8c46aee011f56b7de30a508c6fcb85) |
| 11 | **LP deposits 129.54 USDC** and receives oLP at NAV | Stellar | [`a93c072e…`](https://stellar.expert/explorer/testnet/tx/a93c072ea8726b39af345a57ce241e04839a6fb48eb77e18067d05b827e14049) |
| 12 | Manager allocates 43.18 USDC to **Blend v2** | Stellar | [`9126145f…`](https://stellar.expert/explorer/testnet/tx/9126145f713e703c5634a13d87b44e455a1c56a54ae288e03c7182c473501b39) |
| 13 | Manager allocates 43.18 USDC to **DeFindex** | Stellar | [`689c9f90…`](https://stellar.expert/explorer/testnet/tx/689c9f90340efd572619bed7c330626fbf35f56600a6a5dbedd837af20718d05) |
| 14 | **Router Facility leg:** a seller sells RWA for 75.02 USDC (on-chain quote matches the SDK preview net of the 50 bps fee); the RWA goes straight into the RWA vault, and the payout draws 10.97 USDC back from Blend because idle cash ran short; the discount vests to LPs | Stellar | [`52aa83f4…`](https://stellar.expert/explorer/testnet/tx/52aa83f44d75bc729b7c29a4f769351c199b9578d769c23415862ad3b3e09585) |
| 15 | **Buy back:** a buyer buys RWA from the facility through the facility aggregator | Stellar | [`746bdf0d…`](https://stellar.expert/explorer/testnet/tx/746bdf0d199f28897783836aa5fdb19e85907ca422cb390860217a6c1a951d82) |
| 16 | **Redemption booked:** the curator books 32.07 RWA to the issuer, carried at the oracle mark (33.81 USDC)¹ | Stellar | [`b943a878…`](https://stellar.expert/explorer/testnet/tx/b943a87809045c2f99e2dd1edbb63e3441885ba9273b780904c84742b03f734a) |
| 17 | **Redemption settled:** the issuer's 33.85 USDC proceeds land in the facility; the receivable closes | Stellar | [`4ffdd06a…`](https://stellar.expert/explorer/testnet/tx/4ffdd06a9bf7354c44a28a4ff52088b2dc1935c46294f29a378c822be3ef4836) |
| 18 | Withdrawal queue created | Stellar | [`6f7b9b2c…`](https://stellar.expert/explorer/testnet/tx/6f7b9b2c7d4029ea64b283d69b455cbdbfe35ba1da1bfdb1b77cf2a8d1dbcb2d) |
| 19 | Timelock: queue the queue on the facility | Stellar | [`80ffd1cf…`](https://stellar.expert/explorer/testnet/tx/80ffd1cfa8b2b03b012d32fbd987821b1acf7f62876435b1a5dee7f229ea3aa3) |
| 20 | Timelock: accept it | Stellar | [`58307f2b…`](https://stellar.expert/explorer/testnet/tx/58307f2bf077c2c1b8d32f95369de935c68556944882b4cafe5fba0209813202) |
| 21 | LP requests an exit through the queue | Stellar | [`bb1fba84…`](https://stellar.expert/explorer/testnet/tx/bb1fba845928e90d5183a11295fa047206769a601cdba6c3e8c80f75bfd5a99e) |
| 22 | A third party serves the queue, first come first served; the LP is paid | Stellar | [`15376871…`](https://stellar.expert/explorer/testnet/tx/15376871f630b15f43299cfb6b0f0eaa302c17449e94980c9a0d9d1e9e689e48) |
| 23 | **LP exit:** 15.86 USDC withdrawn; the vault pulls cash back from DeFindex and Blend | Stellar | [`68640c15…`](https://stellar.expert/explorer/testnet/tx/68640c1539623f78094d918669dbbb52c33c3e610281f2681e228cae38a4fa66) |
| 24 | **Curator pause** (instant, no timelock): the quote for 1 RWA drops from 1.0228 USDC to 0, and a sale attempted while paused is refused with `Error(Contract, #15)` = `Paused` | Stellar | [`82cebe03…`](https://stellar.expert/explorer/testnet/tx/82cebe03a34bbba64210b23461888f7b0e06c5fa854ac781ad6bd8ab71b21da8) |
| 25 | LP withdraws 1 USDC **while paused**: a pause never blocks exits | Stellar | [`0aff52a0…`](https://stellar.expert/explorer/testnet/tx/0aff52a0f45719fb0c4349835d3b9deae8d1f0176336eabf2ccf86e7cf4f1d8f) |
| 26 | Curator resumes trading; the quote is back to 1.0228 USDC | Stellar | [`06d1308d…`](https://stellar.expert/explorer/testnet/tx/06d1308d1d0cffd2bbd52ed530d833872e52a9d18cc8ca4575eed9fa10d47778) |

¹ The test suite's own `book` transaction
([`8e59919e…`](https://stellar.expert/explorer/testnet/tx/8e59919eae55abe26bfd504d6b6e70f7fb26a5f1e364f6b0532fb9164cf75023))
failed with `INVOKE_HOST_FUNCTION_RESOURCE_LIMIT_EXCEEDED`: the transaction ran over the resources its
simulation estimated, so it was not a contract error. After the suite finished, the same calls were
resubmitted by hand against the same facility and succeeded: rows 16–17 are that retry, shown in their
logical place. All 25 checks in the suite's other steps passed.

## 5. Redemption vault

A redemption vault [`CDR5AR…XOUH`](https://stellar.expert/explorer/testnet/contract/CDR5AR3SKI5Q5F2NUKN4OYK5ZWGJNWQG4KSS6ZCFI7E3KSE7KDYVXOUH) for tTBILL, paying out in Circle USDC and priced by the push oracle,
with a call verifier [`CCHTTP…ZLZK`](https://stellar.expert/explorer/testnet/contract/CCHTTP5HYHSKXXYQJH3UJTKJXTQ2PEHZNUP7GD3FVVBU3J62MDXOZLZK) allowlisting the issuer calls. Each batch issues its own claim
token (SEP-41), which holders can trade while they wait. Fee: 10 bps. NAV floor for MANUAL
settlement: 50 bps under NAV.

### 5.1 Deployment

| # | What happens | Chain | Transaction |
|---|---|---|---|
| 1 | Upload the claim-token code (one claim token per batch) | Stellar | [`818f6bb1…`](https://stellar.expert/explorer/testnet/tx/818f6bb1e24e77003ee6651d6960da809ba6ea1988358350d345404987409a76) |
| 2 | Upload the redemption-account code (issuer-facing accounts at predictable addresses) | Stellar | [`b9118091…`](https://stellar.expert/explorer/testnet/tx/b91180911dcd1ca7f220e27f540ae0e8687f4a2d9685510431e99472a136360f) |
| 3 | Deploy the call verifier | Stellar | [`7a954b12…`](https://stellar.expert/explorer/testnet/tx/7a954b12853f438a7c3706dfbafd5b5ba7733d2724208b6c9a053a0fa9e8cdb4) |
| 4 | Push oracle: tTBILL NAV 1.00 in Circle USDC | Stellar | [`b487ad65…`](https://stellar.expert/explorer/testnet/tx/b487ad65511c1fd7c08ebbae0bd3d84d853681eb90384a7c4e24be42d95ba138) |
| 5 | Deploy the vault in AUTO mode | Stellar | [`b3ebd3a7…`](https://stellar.expert/explorer/testnet/tx/b3ebd3a75c961a778ca17aa079c8a0527904af55277922c3be3dff48f0e301a9) |
| 6 | Allowlist one issuer call for the vault's accounts: `tTBILL.transfer(<account>, <issuer desk>, <any amount>)` | Stellar | [`9a8fb99f…`](https://stellar.expert/explorer/testnet/tx/9a8fb99f066c5e0a8405f484359501404bf9e0096b1fc978651788911dcee015) |

### 5.2 AUTO batch: the settler redeems through verified issuer calls

| # | What happens | Chain | Transaction |
|---|---|---|---|
| 1 | Holder adds a Circle USDC trustline | Stellar | [`d22cff45…`](https://stellar.expert/explorer/testnet/tx/d22cff4597e44eb88ecab124a402bae16238dbf4dec1b4af58cd21ae7916d035) |
| 2 | Holder requests redemption of 3 tTBILL and receives 3 claim tokens of batch 1 | Stellar | [`74713ef2…`](https://stellar.expert/explorer/testnet/tx/74713ef298e78a7aef66de1a53bd817ca43c9a5cd089b9a64ff6e30008081db5) |
| 3 | Holder cancels 1 tTBILL while the batch is open: claim tokens burned, tTBILL returned | Stellar | [`2a259aeb…`](https://stellar.expert/explorer/testnet/tx/2a259aebd20891b45ac839a2414907607c8369b73fbb52316c9d4635f6cd55f7) |
| 4 | Holder sells 0.5 claim token to another account (secondary trade of claim token [`CCBKXF…5FYX`](https://stellar.expert/explorer/testnet/contract/CCBKXFUHRTG2YKG4MOAZLUP2RFYBXRWSSLNXYOVIPQRY6HPQYT4V5FYX)) | Stellar | [`9144ecba…`](https://stellar.expert/explorer/testnet/tx/9144ecba6598c52a9f5192398f00d5ee321fbde95b7fee386a33587991116dd8) |
| 5 | Settler sends the batch (2 tTBILL) to the issuer from a redemption account; the verifier checks the call and the account's tTBILL must fall by exactly the batch | Stellar | [`5f3fc14b…`](https://stellar.expert/explorer/testnet/tx/5f3fc14b9351cca11e324356eb0a80a3241ce0c5f90709a3f8ac875f5a617865) |
| 6 | Issuer pays 2 USDC into the redemption account | Stellar | [`efc6ce79…`](https://stellar.expert/explorer/testnet/tx/efc6ce797c38b1c2d022e342ccffbb7776056ed054c58825ef56dbd7859f2ea9) |
| 7 | Settler settles: the vault books the USDC that actually arrived (it cannot be overstated) and takes the 10 bps fee | Stellar | [`16fa0eaa…`](https://stellar.expert/explorer/testnet/tx/16fa0eaa1eed11a63fac5e59c150080884644c47d776f0e918c17f22425200ca) |
| 8 | Holder claims with 1.5 claim tokens | Stellar | [`a5fd4412…`](https://stellar.expert/explorer/testnet/tx/a5fd4412599b7532c9ee72e36a07590f9ad281b7d7afdacb8ef86b5fae43c9e0) |
| 9 | The buyer of the 0.5 claim token claims its share | Stellar | [`0743e5af…`](https://stellar.expert/explorer/testnet/tx/0743e5af993f563fba3f3a46064977557e4da5f381a22c527a2a2c0531ee7ea2) |

### 5.3 MANUAL batch: the operator redeems off-chain against a NAV floor

| # | What happens | Chain | Transaction |
|---|---|---|---|
| 1 | Governance: queue the switch to MANUAL (timelocked; testnet delay 0) | Stellar | [`d05415e4…`](https://stellar.expert/explorer/testnet/tx/d05415e4677983fd243abd1de46450c22d04e5831f070a96cda78ca804f63b4c) |
| 2 | Governance: accept the switch | Stellar | [`f01bc529…`](https://stellar.expert/explorer/testnet/tx/f01bc5297398c835f2b05221e9d5a941350081c5c9d8dbed9f0aa20d8a27e2d0) |
| 3 | Holder requests redemption of 1 tTBILL (batch 2) | Stellar | [`22181644…`](https://stellar.expert/explorer/testnet/tx/22181644dd86b8f9275241ad12a033118334e81ae224092c6f23ee6ce78517fe) |
| 4 | Operator takes the batch to an allowed issuer destination | Stellar | [`86ad0a63…`](https://stellar.expert/explorer/testnet/tx/86ad0a63847c9cc73cb1ec527e088f23d3e8b6ef9362a484b55c4b0bf6c8b432) |
| 5 | Operator settles: the issuer redeemed 0.75 and **returned 0.25 tTBILL in kind**; 0.75 USDC paid in and checked against the NAV floor on the 0.75 redeemed | Stellar | [`25dbd7f7…`](https://stellar.expert/explorer/testnet/tx/25dbd7f7026fa4c83a882a3ea67de7906d29a35d50603b50ade1deb245c517d0) |
| 6 | Holder claims its USDC and its share of the returned tTBILL | Stellar | [`70419b3c…`](https://stellar.expert/explorer/testnet/tx/70419b3c381137f68ac4bb0f37bdb901b2756de7581d0f2913c040f31b4c5c63) |

## 6. Cross-chain RFQ: Stellar ↔ Sepolia over CCTP

`cctp_settle` [`CD7UQ4…XF6Q`](https://stellar.expert/explorer/testnet/contract/CD7UQ4NZOCIYXL55V2CAZG2SWSGEBWY6VNH6SJOOCSYMNXKGTOWMXF6Q) on Stellar (CCTP domain 27) and
`CctpSettle` [`0xcef519…aeec`](https://sepolia.etherscan.io/address/0xcef519e9f5fdb966bef637662c59fc4de8a7aeec) on Sepolia (domain 0). FILL and FAIL travel as
CCTP messages, and payments as CCTP USDC burns. CCTP has no executor, so each leg is relayed by
whoever is paid to: the maker's keeper relays FILL, the taker relays FAIL, and anyone can `complete`
a payment for a flat **0.50 USDC tip** (now the same on both chains). Circle attests a Stellar message
in seconds and a Sepolia message after Ethereum finality (about 18 minutes), which is most of each
trade's wall-clock time.

Runs 6.1 and 6.2 go through the **Octarine backend API**, making the same calls the UI makes:

1. **Request.** The taker opens a request with "Allow cross-chain" (`crossChain: {}`). It is open to
   makers on every chain paired with the taker's.
2. **Bid.** Each maker picks the chain it bids from; that pair's bridge is chosen off-chain.
3. **Transactions.** The API builds and verifies every transaction and tracks each leg through
   Circle's attestation API.

### 6.1 DELIVER: Stellar taker → Sepolia maker, through the API

Request `b45810e1…`: 10 tTBILL sold to a maker bidding from Sepolia at 2.5 bps/day. The maker pays
9.925 USDC on Sepolia; the taker nets 9.415 USDC after the 0.0099 fee and the 0.50 tip, and keeps the
tip by completing itself. The tTBILL is delivered to the recipient on Stellar.

| # | What happens | Chain | Transaction |
|---|---|---|---|
| 1 | Taker fills on Stellar: tTBILL escrowed, FILL sent as a CCTP message to Sepolia | Stellar | [`eacc8475…`](https://stellar.expert/explorer/testnet/tx/eacc8475618c129651eb2d821df9811b6b0311260c88aebe29e071882e74ca64) |
| 2 | Maker's keeper relays the FILL on Sepolia (`receiveMessage`): the signed bid is checked, the maker's USDC pulled and burned back to Stellar with PAID | Sepolia | [`0xaf04793a…`](https://sepolia.etherscan.io/tx/0xaf04793a722fbd963d45d1bc57dd6cc5a26b434eb73cbc616affb791dcc91434) |
| 3 | Taker completes on Stellar after Sepolia finality: **SETTLED**; taker paid, fee and tip out, 10 tTBILL to the recipient | Stellar | [`32f75189…`](https://stellar.expert/explorer/testnet/tx/32f75189caf4cd6ba64cdf983f789e5297c2d3cb1ecae93c409c29bb6a4da272) |

### 6.2 DELIVER: Sepolia taker → Stellar maker, through the API

Request `9f49d83e…`: 10 tACRED sold on Sepolia to a maker bidding from Stellar. The request was opened
to every paired chain, and the maker picked Stellar when it bid. Because Soroban forbids the reply
re-entering Circle's transmitter, the relay into Stellar has two steps. The taker's USDC on Sepolia
rose by the full 9.925: the taker's wallet is also the Sepolia fee recipient, and it completed the
trade itself, so it got the fee and the tip back.

| # | What happens | Chain | Transaction |
|---|---|---|---|
| 1 | Maker (Stellar) approves `cctp_settle` for Circle USDC — the approval the API built | Stellar | [`f2e44d27…`](https://stellar.expert/explorer/testnet/tx/f2e44d279777196b671f39ee0645cde4ba55fb19804575324494fea8192bff87) |
| 2 | Taker (Sepolia) approves CctpSettle for 10 tACRED | Sepolia | [`0xa5081683…`](https://sepolia.etherscan.io/tx/0xa5081683762aa567f47dbfa0ed471402855bfadd2f66a29771b5cea8f784d742) |
| 3 | Taker fills on Sepolia — the request was open to every paired chain; the maker bid from Stellar, so the fill goes to domain 27 | Sepolia | [`0xe1f1d0b4…`](https://sepolia.etherscan.io/tx/0xe1f1d0b4401a96ca0ee847bd3b101b24ec51b7e7b32723471825c922ad1a5081) |
| 4 | Maker relays the FILL on Stellar, step 1: `receive_message` on Circle's transmitter | Stellar | [`8004f1e7…`](https://stellar.expert/explorer/testnet/tx/8004f1e7227e48ebadfbd37472415bfa90eb23670be62cd2ba029c54955962ed) |
| 5 | Maker relays the FILL on Stellar, step 2: `cctp_settle.execute` pulls the maker's 9.925 USDC and burns it to Sepolia with PAID | Stellar | [`6dea9786…`](https://stellar.expert/explorer/testnet/tx/6dea97865525e1a79390d7d5dc64d5690d38f4428194ee0a92c57c0bedfe6203) |
| 6 | Taker completes on Sepolia — SETTLED: taker paid, fee and tip out, 10 tACRED delivered to the recipient | Sepolia | [`0xea5d6017…`](https://sepolia.etherscan.io/tx/0xea5d601711fb549e6fc1e8d09e0393db0f4a8ab341f88e8bfe6c2c2ff71a6633) |

### 6.3 REDEEM_CT: the RWA is redeemed and the claim token delivered

The maker bids in REDEEM_CT mode, so instead of receiving tTBILL it receives the redemption vault's
claim token (5 tTBILL's worth) on Stellar. It can hold the token or sell it. Run directly against
the contracts: the API created and priced the request and quoted the bid, but it only offers
DELIVER, so this bid's delivery mode was set before signing.

| # | What happens | Chain | Transaction |
|---|---|---|---|
| 1 | Governance: queue the tTBILL redemption vault on `cctp_settle` (enables the REDEEM modes) | Stellar | [`15db84b2…`](https://stellar.expert/explorer/testnet/tx/15db84b2d2ab770449a462853968bdb4ff8efa7b0f1d93ebdb81d25de8c5a67c) |
| 2 | Governance: accept it | Stellar | [`bda94702…`](https://stellar.expert/explorer/testnet/tx/bda9470250b2590b08166ff31cfadcac24d17c621eb92b21c3c33cf2ac2a071f) |
| 3 | Taker fills on Stellar: 5 tTBILL escrowed, FILL leaves as a CCTP message | Stellar | [`51e21ebe…`](https://stellar.expert/explorer/testnet/tx/51e21ebe8557ac99af3ffeb96cdac9858f9ae62660bdaa12bdeed6d47e3064ef) |
| 4 | Maker's keeper relays the FILL on Sepolia: 4.9625 USDC pulled and burned home with PAID | Sepolia | [`0x51d3ce03…`](https://sepolia.etherscan.io/tx/0x51d3ce0320476e765a81eb1f500dd0e85748c7b1ad88ba99c917518eb161f49a) |
| 5 | Taker completes on Stellar: paid 4.4575 USDC net plus the 0.50 tip it earned; the 5 tTBILL go into the vault's open batch, and **5 claim tokens go to the recipient** | Stellar | [`4c41e55e…`](https://stellar.expert/explorer/testnet/tx/4c41e55eed5d867e7a1d3241a8fd2dfdff711a282809fe193eddee198215eaf2) |
| 6 | Later, once the batch is paid (6.5), the recipient redeems its 5 claim tokens for 4.995 USDC | Stellar | [`10c1b885…`](https://stellar.expert/explorer/testnet/tx/10c1b8856b52e8da4b5ae8f90f06d2f2947f1abd1e047e5e4a53f5da8d74b0af) |

### 6.4 REDEEM_PUSH: redeemed, then the proceeds are pushed to the maker's chain

The maker wants USDC back on Sepolia rather than a Stellar claim token. `cctp_settle` holds the
claim; once the issuer pays, anyone calls `push`, which claims the proceeds and burns them to the
maker on Sepolia as PAYOUT.

| # | What happens | Chain | Transaction |
|---|---|---|---|
| 1 | Taker fills on Stellar: 5 tTBILL escrowed | Stellar | [`9b9943d0…`](https://stellar.expert/explorer/testnet/tx/9b9943d022fc31933dc261c852ce179fbf7d8ed7186ae40a6342876fbe38f526) |
| 2 | Maker's keeper relays the FILL on Sepolia: 4.9625 USDC pulled and burned home with PAID | Sepolia | [`0x83d102f9…`](https://sepolia.etherscan.io/tx/0x83d102f91b995417c1c81c3da16fff8b08f0f0eb45b8af62293500b8332d0d9a) |
| 3 | Taker completes on Stellar: taker paid; the 5 tTBILL go into the open batch, and **`cctp_settle` holds the claim for the maker** | Stellar | [`508f62bc…`](https://stellar.expert/explorer/testnet/tx/508f62bcaa1d707ce9bfca2404ea1d1405b6b2259cb0359c72e55fd85c54632d) |
| 4 | Once the batch is paid (6.5), anyone calls `push`: `cctp_settle` claims 4.995 USDC from the vault and burns it to the maker as PAYOUT | Stellar | [`878ddf3d…`](https://stellar.expert/explorer/testnet/tx/878ddf3d3c7b9958d4ca662cbf6195908d0efc2b09e3fe83984a657f75d4e822) |
| 5 | PAYOUT completed on Sepolia: **the maker receives 4.995 USDC**, the redemption proceeds less the vault's 10 bps fee | Sepolia | [`0x8695c6fb…`](https://sepolia.etherscan.io/tx/0x8695c6fbe3353f0ce78daa5e57704ac045e878da64412e62f184e97ec3ada663) |

### 6.5 The redemption batch behind 6.3 and 6.4

Both cross-chain fills pooled into one vault batch (batch 3, 10 tTBILL). The claim tokens were split
5 / 5 between `cctp_settle` (REDEEM_PUSH) and the REDEEM_CT recipient. After the steps below the
batch has nothing left and its claim-token supply is 0.

| # | What happens | Chain | Transaction |
|---|---|---|---|
| 1 | Operator takes batch 3 to the issuer | Stellar | [`66cdc970…`](https://stellar.expert/explorer/testnet/tx/66cdc9709f093d0ddc6511167b09fda4104c053a0a88bfefdccd824c3f8daec2) |
| 2 | Operator tries to settle 1% under NAV, below the 50 bps floor: refused with `Error(Contract, #8)` = `ProceedsTooLow` (simulated, not submitted) | Stellar | — (simulated) |
| 3 | Operator settles batch 3 at NAV: 10 USDC in, 10 bps fee out | Stellar | [`73099a6a…`](https://stellar.expert/explorer/testnet/tx/73099a6a508a026791d1640d44baf8eb86c79840f39fa7cf2346cc4e35135818) |

### 6.6 Maker reneges: FAIL, and the taker takes its RWA back

The maker signs a DELIVER bid, the taker fills, and then the maker cancels the bid's salt on Sepolia
before relaying. When the FILL lands, `CctpSettle` refuses the bid and answers FAIL, emitting
`Failed`. The taker relays the FAIL on Stellar and its 3 tTBILL are returned. The order is closed
unfilled, so the request can take another bid.

| # | What happens | Chain | Transaction |
|---|---|---|---|
| 1 | Taker fills on Stellar: 3 tTBILL escrowed | Stellar | [`34c3e6af…`](https://stellar.expert/explorer/testnet/tx/34c3e6aff9a314c65396c1672ef815a2b9b6997bc3cdc04c2ef7163786e04e2e) |
| 2 | Maker cancels its bid's salt on the Sepolia RFQ | Sepolia | [`0xcf536282…`](https://sepolia.etherscan.io/tx/0xcf536282d1b7869e423bb2a529c020bb0fa7309a4178a7a61e75746ee4efba94) |
| 3 | The FILL is relayed on Sepolia: the bid is refused and FAIL goes back (event `Failed`, not `Paid`) | Sepolia | [`0xceee232a…`](https://sepolia.etherscan.io/tx/0xceee232a2c223924e48c24bd2d6432d1249e6347ac7ce70db463b63579387b2b) |
| 4 | Taker relays the FAIL on Stellar (Circle transmitter `receive_message`): **3 tTBILL returned** | Stellar | [`bc9fdb20…`](https://stellar.expert/explorer/testnet/tx/bc9fdb2097b1d7af59853c71ff2a6afc116d9fb32b081d7443537df8f995f62f) |

### 6.7 CROSS_DELIVER: refused on CCTP, by design

CROSS_DELIVER bridges the RWA itself to the maker's chain. CCTP burns only Circle's own tokens, so
`cctp_settle` refuses it at `fill` whatever the routes say. A real signed CROSS_DELIVER bid,
simulated as the taker's fill, is rejected with `HostError: Error(Contract, #3)` = `NoRoute`. No transaction was
submitted.

CROSS_DELIVER needs a LayerZero OFT for the RWA on both chains. No RWA has one on testnet, and
neither does USDT0. Until one does, the LayerZero contracts (`lz_settle` / `LzSettle`) are deployed
and peered but open no route. The LayerZero message path was exercised live by an earlier
deployment (below), and the USDT0 path is covered by mainnet-fork tests.

### 6.8 Configuration transactions

| # | What happens | Chain | Transaction |
|---|---|---|---|
| 1 | Sepolia `CctpSettle.setTip(500000)`: the relay tip raised from 0.10 to 0.50 USDC to match Stellar | Sepolia | [`0x4ce849e5…`](https://sepolia.etherscan.io/tx/0x4ce849e5d77c857149c046337ee5b37e2a78eab9c6d60396fa18be44ff807b8f) |

## Earlier runs, still on-chain

| Run | Transactions |
|---|---|
| LayerZero FILL (retired combined contract, 2026-10-23): FILL over LayerZero, USDC over CCTP | fill [`236d5e5b…`](https://stellar.expert/explorer/testnet/tx/236d5e5bc21f93c4cd3c1eb8cbff1f201ae361f57a96492823f7f7b3d2b19ce0) → re-executed on Sepolia via `EndpointV2.lzReceive` [`0x4fd17e9b…`](https://sepolia.etherscan.io/tx/0x4fd17e9b6628596c13607d83ac2bf162cc5cb1aedee715688b2a1ce1bfde4349) → complete [`fc59f9d6…`](https://stellar.expert/explorer/testnet/tx/fc59f9d6eeeb039de9b6ed293914748bf8f51dfda812bf52a9ce75404795cf7b) |
| CCTP DELIVER, hand-run on the current contracts (2026-10-30) | fill [`0c28aa69…`](https://stellar.expert/explorer/testnet/tx/0c28aa699810315cd099f82526e9ece66412e85d5277992ab19b97f85a45beb8) → relay [`0xbd72aba0…`](https://sepolia.etherscan.io/tx/0xbd72aba0383ef15fb3ee2412287ff4a7dc01795b39a0caca9bc280ae9c26c0b5) → complete [`4ca70146…`](https://stellar.expert/explorer/testnet/tx/4ca70146f93a87e263a994ec990a9f7d0a96629d0ce092322742c635619eb880) |

More runs from Milestones 1 and 2 are listed in [DEPLOYMENTS.md](DEPLOYMENTS.md).

## Reproducing

- **Sections 1–4.** Run `./scripts/05-fill-demo.sh`, then
  `cd sdk && ADMIN_SECRET=… MAKER_SECRET=… TAKER_SECRET=… npm run test:testnet`. The suite writes
  every hash to `deployments/testnet-integration.json`.
- **Cross-chain.** Each step is described in
  [CROSSCHAIN_SETTLEMENT.md](CROSSCHAIN_SETTLEMENT.md) and in the hand-testing guide in
  [DEPLOYMENTS.md](DEPLOYMENTS.md#testing-it-by-hand-the-cctp-route-a-stellar-taker-and-a-sepolia-maker).
- **Testnet only.** Governance delays are 0. tTBILL's NAV is pushed by the admin, and the Sepolia RWA
  (tACRED) is priced by a mock oracle.
