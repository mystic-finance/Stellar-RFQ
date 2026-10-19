//! Real auth throughout: only the RFQ router's signature on the aggregator is
//! mocked, so the nested token pull succeeds only if the adapter authorised
//! exactly that transfer.

use super::*;
use dex_aggregator::{DexAggregator, DexAggregatorClient, FeeConfig, Hop, Split};
use soroban_sdk::testutils::{Address as _, MockAuth, MockAuthInvoke};
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::xdr::ToXdr;

/// Behaves as the deployed router does: `to.require_auth()`, then pulls the
/// input from `to` into the pair inside its own call. It doubles as the pair.
/// `greed` makes it pull more than it was asked to.
#[contract]
pub struct MockRouter;

#[contractimpl]
impl MockRouter {
    pub fn __constructor(env: Env, rate_bps: i128, greed: i128) {
        env.storage()
            .instance()
            .set(&symbol(&env, "rate"), &rate_bps);
        env.storage().instance().set(&symbol(&env, "greed"), &greed);
    }

    pub fn swap_exact_tokens_for_tokens(
        e: Env,
        amount_in: i128,
        amount_out_min: i128,
        path: Vec<Address>,
        to: Address,
        deadline: u64,
    ) -> Vec<i128> {
        assert!(e.ledger().timestamp() < deadline, "deadline");
        to.require_auth();
        let pair = Self::router_pair_for(e.clone(), path.get(0).unwrap(), path.get(1).unwrap());
        let greed: i128 = e.storage().instance().get(&symbol(&e, "greed")).unwrap();
        TokenClient::new(&e, &path.get(0).unwrap()).transfer(&to, &pair, &(amount_in + greed));
        let out = Self::out(&e, amount_in);
        assert!(out >= amount_out_min, "min");
        TokenClient::new(&e, &path.get(1).unwrap()).transfer(&pair, &to, &out);
        vec![&e, amount_in, out]
    }

    pub fn router_get_amounts_out(e: Env, amount_in: i128, _path: Vec<Address>) -> Vec<i128> {
        vec![&e, amount_in, Self::out(&e, amount_in)]
    }

    pub fn router_pair_for(e: Env, _a: Address, _b: Address) -> Address {
        e.current_contract_address()
    }

    fn out(e: &Env, amount_in: i128) -> i128 {
        let rate: i128 = e.storage().instance().get(&symbol(e, "rate")).unwrap();
        amount_in * rate / 10_000
    }
}

fn symbol(env: &Env, s: &str) -> Symbol {
    Symbol::new(env, s)
}

struct Fixture {
    env: Env,
    agg: DexAggregatorClient<'static>,
    adapter: Address,
    router_signer: Address,
    a: Address,
    c: Address,
}

fn setup(greed: i128) -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    env.cost_estimate().budget().reset_unlimited();
    let issuer = Address::generate(&env);
    let a = env
        .register_stellar_asset_contract_v2(issuer.clone())
        .address();
    let c = env.register_stellar_asset_contract_v2(issuer).address();
    let router_signer = Address::generate(&env);
    let agg_id = env.register(
        DexAggregator,
        (
            Address::generate(&env),
            router_signer.clone(),
            FeeConfig {
                recipient: Address::generate(&env),
                protocol_fee_bps: 0,
                max_partner_fee_bps: 0,
                partner_share_bps: 0,
            },
        ),
    );
    let soroswap = env.register(MockRouter, (9_970i128, greed));
    StellarAssetClient::new(&env, &c).mint(&soroswap, &1_000_000_000);
    StellarAssetClient::new(&env, &a).mint(&soroswap, &1_000_000_000);
    let adapter = env.register(SoroswapAdapter, (agg_id.clone(), soroswap));
    let agg = DexAggregatorClient::new(&env, &agg_id);
    agg.allow_venue(&adapter, &true);
    Fixture {
        env,
        agg,
        adapter,
        router_signer,
        a,
        c,
    }
}

impl Fixture {
    fn route(&self, amount: i128) -> Vec<Split> {
        vec![
            &self.env,
            Split {
                amount_in: amount,
                hops: vec![
                    &self.env,
                    Hop {
                        venue: self.adapter.clone(),
                        token_out: self.c.clone(),
                        data: Bytes::new(&self.env),
                    },
                ],
            },
        ]
    }

    /// The RFQ router pushes the input and calls `fill`; only its signature is mocked.
    fn fill(&self, recipient: &Address, amount: i128) -> Result<i128, ()> {
        StellarAssetClient::new(&self.env, &self.a).mint(&self.agg.address, &amount);
        let data = dex_aggregator::Route::Splits(self.route(amount)).to_xdr(&self.env);
        self.env.mock_auths(&[MockAuth {
            address: &self.router_signer,
            invoke: &MockAuthInvoke {
                contract: &self.agg.address,
                fn_name: "fill",
                args: (
                    recipient.clone(),
                    self.a.clone(),
                    self.c.clone(),
                    amount,
                    1i128,
                    data.clone(),
                )
                    .into_val(&self.env),
                sub_invokes: &[],
            },
        }]);
        match self
            .agg
            .try_fill(recipient, &self.a, &self.c, &amount, &1, &data)
        {
            Ok(Ok(v)) => Ok(v),
            _ => Err(()),
        }
    }
}

#[test]
fn swaps_through_soroswap_under_real_nested_auth() {
    let f = setup(0);
    let recipient = Address::generate(&f.env);
    assert_eq!(f.fill(&recipient, 1_000_000), Ok(997_000));
    assert_eq!(TokenClient::new(&f.env, &f.c).balance(&recipient), 997_000);
    // The adapter keeps nothing.
    assert_eq!(TokenClient::new(&f.env, &f.a).balance(&f.adapter), 0);
    assert_eq!(TokenClient::new(&f.env, &f.c).balance(&f.adapter), 0);
}

#[test]
fn soroswap_cannot_pull_more_than_the_hop_authorised() {
    let f = setup(1);
    let recipient = Address::generate(&f.env);
    assert_eq!(f.fill(&recipient, 1_000_000), Err(()));
}

#[test]
fn the_quote_is_the_routers_amounts_out() {
    let f = setup(0);
    let q = SoroswapAdapterClient::new(&f.env, &f.adapter).quote(
        &f.a,
        &f.c,
        &1_000_000,
        &Bytes::new(&f.env),
    );
    assert_eq!(q, 997_000);
}

#[test]
fn only_the_aggregator_can_swap() {
    let f = setup(0);
    f.env.set_auths(&[]);
    let stranger = Address::generate(&f.env);
    assert!(SoroswapAdapterClient::new(&f.env, &f.adapter)
        .try_swap(&f.a, &f.c, &1_000, &1, &Bytes::new(&f.env), &stranger)
        .is_err());
}
