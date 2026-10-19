//! Real auth throughout: only the RFQ router's signature on the aggregator is
//! mocked, so the pool's pull succeeds only if the adapter authorised exactly
//! that transfer.

use super::*;
use dex_aggregator::{DexAggregator, DexAggregatorClient, FeeConfig, Hop, Split};
use soroban_sdk::testutils::{Address as _, MockAuth, MockAuthInvoke};
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::xdr::ToXdr;

/// Behaves as a deployed Aquarius pool does: `user.require_auth()`, pulls the
/// input from `user`, pays the output to `user`. `greed` over-pulls.
#[contract]
pub struct MockPool;

#[contractimpl]
impl MockPool {
    pub fn __constructor(env: Env, tokens: Vec<Address>, rate_bps: u128, greed: i128) {
        env.storage()
            .instance()
            .set(&symbol(&env, "tokens"), &tokens);
        env.storage()
            .instance()
            .set(&symbol(&env, "rate"), &rate_bps);
        env.storage().instance().set(&symbol(&env, "greed"), &greed);
    }

    pub fn swap(
        e: Env,
        user: Address,
        in_idx: u32,
        out_idx: u32,
        in_amount: u128,
        out_min: u128,
    ) -> u128 {
        user.require_auth();
        let tokens = Self::get_tokens(e.clone());
        let me = e.current_contract_address();
        let greed: i128 = e.storage().instance().get(&symbol(&e, "greed")).unwrap();
        TokenClient::new(&e, &tokens.get(in_idx).unwrap()).transfer(
            &user,
            &me,
            &(in_amount as i128 + greed),
        );
        let out = Self::estimate_swap(e.clone(), in_idx, out_idx, in_amount);
        assert!(out >= out_min, "min");
        TokenClient::new(&e, &tokens.get(out_idx).unwrap()).transfer(&me, &user, &(out as i128));
        out
    }

    pub fn estimate_swap(e: Env, _in_idx: u32, _out_idx: u32, in_amount: u128) -> u128 {
        let rate: u128 = e.storage().instance().get(&symbol(&e, "rate")).unwrap();
        in_amount * rate / 10_000
    }

    pub fn get_tokens(e: Env) -> Vec<Address> {
        e.storage().instance().get(&symbol(&e, "tokens")).unwrap()
    }
}

fn symbol(env: &Env, s: &str) -> Symbol {
    Symbol::new(env, s)
}

struct Fixture {
    env: Env,
    agg: DexAggregatorClient<'static>,
    adapter: Address,
    pool: Address,
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
    let pool = env.register(
        MockPool,
        (vec![&env, a.clone(), c.clone()], 9_970u128, greed),
    );
    StellarAssetClient::new(&env, &a).mint(&pool, &1_000_000_000);
    StellarAssetClient::new(&env, &c).mint(&pool, &1_000_000_000);
    let adapter = env.register(AquariusAdapter, (agg_id.clone(),));
    let agg = DexAggregatorClient::new(&env, &agg_id);
    agg.allow_venue(&adapter, &true);
    Fixture {
        env,
        agg,
        adapter,
        pool,
        router_signer,
        a,
        c,
    }
}

impl Fixture {
    fn pool_data(&self) -> Bytes {
        self.pool.clone().to_xdr(&self.env)
    }

    fn adapter(&self) -> AquariusAdapterClient<'_> {
        AquariusAdapterClient::new(&self.env, &self.adapter)
    }

    /// The RFQ router pushes the input and calls `fill`; only its signature is mocked.
    fn fill(&self, recipient: &Address, amount: i128) -> Result<i128, ()> {
        StellarAssetClient::new(&self.env, &self.a).mint(&self.agg.address, &amount);
        let route = vec![
            &self.env,
            Split {
                amount_in: amount,
                hops: vec![
                    &self.env,
                    Hop {
                        venue: self.adapter.clone(),
                        token_out: self.c.clone(),
                        data: self.pool_data(),
                    },
                ],
            },
        ];
        let data = dex_aggregator::Route::Splits(route).to_xdr(&self.env);
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
fn swaps_through_an_aquarius_pool_under_real_nested_auth() {
    let f = setup(0);
    let recipient = Address::generate(&f.env);
    assert_eq!(f.fill(&recipient, 1_000_000), Ok(997_000));
    assert_eq!(TokenClient::new(&f.env, &f.c).balance(&recipient), 997_000);
    assert_eq!(TokenClient::new(&f.env, &f.a).balance(&f.adapter), 0);
    assert_eq!(TokenClient::new(&f.env, &f.c).balance(&f.adapter), 0);
}

#[test]
fn the_pool_cannot_pull_more_than_the_hop_authorised() {
    let f = setup(1);
    let recipient = Address::generate(&f.env);
    assert_eq!(f.fill(&recipient, 1_000_000), Err(()));
}

#[test]
fn the_quote_is_the_pools_estimate() {
    let f = setup(0);
    assert_eq!(
        f.adapter().quote(&f.a, &f.c, &1_000_000, &f.pool_data()),
        997_000
    );
}

#[test]
fn malformed_pool_data_and_foreign_tokens_are_rejected() {
    let f = setup(0);
    let adapter = f.adapter();
    assert_eq!(
        adapter.try_quote(&f.a, &f.c, &1, &Bytes::new(&f.env)),
        Err(Ok(Error::InvalidPool.into()))
    );
    let stranger = f
        .env
        .register_stellar_asset_contract_v2(Address::generate(&f.env))
        .address();
    assert_eq!(
        adapter.try_quote(&f.a, &stranger, &1, &f.pool_data()),
        Err(Ok(Error::TokenNotInPool.into()))
    );
    assert_eq!(
        adapter.try_quote(&f.a, &f.c, &-1, &f.pool_data()),
        Err(Ok(Error::InvalidAmount.into()))
    );
}

#[test]
fn only_the_aggregator_can_swap() {
    let f = setup(0);
    f.env.set_auths(&[]);
    let stranger = Address::generate(&f.env);
    assert!(f
        .adapter()
        .try_swap(&f.a, &f.c, &1_000, &1, &f.pool_data(), &stranger)
        .is_err());
}
