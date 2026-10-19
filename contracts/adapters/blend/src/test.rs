//! Against a pool that behaves as Blend v2 does for a plain supplier: `submit`
//! authorises the spender and pulls with a nested `transfer`, mints b-tokens
//! rounding down, burns them rounding up, clamps a withdrawal to the position,
//! and refuses one that would take utilization to 100%. Only the owner's
//! signature is mocked, so the pool's pull succeeds only if the adapter
//! authorised exactly that transfer.

use super::*;
use soroban_sdk::testutils::{Address as _, MockAuth, MockAuthInvoke};
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::{map, symbol_short};

#[contract]
pub struct MockPool;

#[contractimpl]
impl MockPool {
    pub fn __constructor(env: Env, asset: Address) {
        env.storage()
            .instance()
            .set(&symbol_short!("asset"), &asset);
        Self::put(&env, &Self::fresh(&env, asset));
    }

    /// Interest accrued (`b_rate`) and borrowed (`d_supply`, at `d_rate` 1).
    pub fn set_market(env: Env, b_rate: i128, borrowed: i128) {
        let mut r = Self::get_reserve(env.clone(), Self::asset(&env));
        r.data.b_rate = b_rate;
        r.data.d_supply = borrowed;
        Self::put(&env, &r);
    }

    pub fn submit(
        env: Env,
        from: Address,
        spender: Address,
        to: Address,
        requests: Vec<Request>,
    ) -> Positions {
        spender.require_auth();
        if from != spender {
            from.require_auth();
        }
        let me = env.current_contract_address();
        let mut r = Self::get_reserve(env.clone(), Self::asset(&env));
        let mut held = Self::get_positions(env.clone(), from.clone())
            .supply
            .get(0)
            .unwrap_or(0);
        let token = TokenClient::new(&env, &r.asset);
        for q in requests.iter() {
            if q.request_type == SUPPLY {
                token.transfer(&spender, &me, &q.amount);
                let minted = q.amount * SCALAR_12 / r.data.b_rate;
                assert!(minted > 0, "InvalidBTokenMintAmount");
                held += minted;
                r.data.b_supply += minted;
            } else {
                let mut burn = (q.amount * SCALAR_12 + r.data.b_rate - 1) / r.data.b_rate;
                let mut out = q.amount;
                if burn > held {
                    burn = held;
                    out = held * r.data.b_rate / SCALAR_12;
                }
                assert!(burn > 0, "InvalidBTokenBurnAmount");
                held -= burn;
                r.data.b_supply -= burn;
                Self::require_utilization_below_100(&r.data);
                token.transfer(&me, &to, &out);
            }
        }
        Self::put(&env, &r);
        let p = Positions {
            liabilities: Map::new(&env),
            collateral: Map::new(&env),
            supply: map![&env, (0u32, held)],
        };
        env.storage().persistent().set(&from, &p);
        p
    }

    pub fn get_positions(env: Env, address: Address) -> Positions {
        env.storage()
            .persistent()
            .get(&address)
            .unwrap_or(Positions {
                liabilities: Map::new(&env),
                collateral: Map::new(&env),
                supply: Map::new(&env),
            })
    }

    pub fn get_reserve(env: Env, asset: Address) -> Reserve {
        assert!(asset == Self::asset(&env), "no reserve");
        env.storage().instance().get(&symbol_short!("r")).unwrap()
    }

    fn require_utilization_below_100(d: &ReserveData) {
        let supply = d.b_supply * d.b_rate / SCALAR_12;
        let owed = (d.d_supply * d.d_rate + SCALAR_12 - 1) / SCALAR_12;
        if owed == 0 {
            return;
        }
        assert!(owed < supply, "InvalidUtilRate");
        assert!(
            (owed * SCALAR_7 + supply - 1) / supply < SCALAR_7,
            "InvalidUtilRate"
        );
    }

    fn asset(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&symbol_short!("asset"))
            .unwrap()
    }

    fn put(env: &Env, r: &Reserve) {
        env.storage().instance().set(&symbol_short!("r"), r);
    }

    fn fresh(env: &Env, asset: Address) -> Reserve {
        let _ = env;
        Reserve {
            asset,
            config: ReserveConfig {
                index: 0,
                decimals: 7,
                c_factor: 0,
                l_factor: 0,
                util: 0,
                max_util: 0,
                r_base: 0,
                r_one: 0,
                r_two: 0,
                r_three: 0,
                reactivity: 0,
                supply_cap: i128::MAX,
                enabled: true,
            },
            data: ReserveData {
                d_rate: SCALAR_12,
                b_rate: SCALAR_12,
                ir_mod: 0,
                b_supply: 0,
                d_supply: 0,
                backstop_credit: 0,
                last_time: 0,
            },
            scalar: 10_000_000,
        }
    }
}

struct Fixture {
    env: Env,
    adapter: BlendAdapterClient<'static>,
    pool: MockPoolClient<'static>,
    owner: Address,
    usdc: Address,
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    env.cost_estimate().budget().reset_unlimited();
    let usdc = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let pool = env.register(MockPool, (usdc.clone(),));
    let owner = Address::generate(&env);
    let adapter = env.register(BlendAdapter, (owner.clone(), usdc.clone(), pool.clone()));
    Fixture {
        adapter: BlendAdapterClient::new(&env, &adapter),
        pool: MockPoolClient::new(&env, &pool),
        env,
        owner,
        usdc,
    }
}

impl Fixture {
    /// What the owning vault does: push the asset, then call `deposit`, with
    /// only its own signature.
    fn supply(&self, amount: i128) {
        StellarAssetClient::new(&self.env, &self.usdc).mint(&self.adapter.address, &amount);
        self.env.mock_auths(&[MockAuth {
            address: &self.owner,
            invoke: &MockAuthInvoke {
                contract: &self.adapter.address,
                fn_name: "deposit",
                args: (amount,).into_val(&self.env),
                sub_invokes: &[],
            },
        }]);
        self.adapter.deposit(&amount);
        self.env.mock_all_auths();
    }

    fn bal(&self, who: &Address) -> i128 {
        TokenClient::new(&self.env, &self.usdc).balance(who)
    }
}

#[test]
fn supplies_under_real_nested_auth_and_tracks_interest() {
    let f = setup();
    f.supply(1_000_000);
    assert_eq!(f.bal(&f.pool.address), 1_000_000);
    assert_eq!(f.bal(&f.adapter.address), 0);
    assert_eq!(f.adapter.total_assets(), 1_000_000);

    // 5% interest accrues to the b-token rate.
    f.pool.set_market(&(SCALAR_12 * 105 / 100), &0);
    StellarAssetClient::new(&f.env, &f.usdc).mint(&f.pool.address, &50_000);
    assert_eq!(f.adapter.total_assets(), 1_050_000);
    assert_eq!(f.adapter.max_withdraw(), 1_050_000);
}

#[test]
fn withdrawals_pay_the_target_directly_and_clamp_to_the_position() {
    let f = setup();
    f.supply(1_000_000);
    let vault = Address::generate(&f.env);
    assert_eq!(f.adapter.withdraw(&400_000, &vault), 400_000);
    assert_eq!(f.bal(&vault), 400_000);

    // Asking for more than is held takes exactly what is held.
    assert_eq!(f.adapter.withdraw(&i128::MAX, &vault), 600_000);
    assert_eq!(f.bal(&vault), 1_000_000);
    assert_eq!(f.adapter.total_assets(), 0);
    assert_eq!(f.adapter.withdraw(&1, &vault), 0);
}

#[test]
fn instant_liquidity_is_bounded_by_utilization() {
    let f = setup();
    f.supply(1_000_000);
    // 900k borrowed out of the pool.
    f.pool.set_market(&SCALAR_12, &900_000);
    TokenClient::new(&f.env, &f.usdc).transfer(
        &f.pool.address,
        &Address::generate(&f.env),
        &900_000,
    );

    let max = f.adapter.max_withdraw();
    assert!(max > 0 && max < 100_000, "max {max}");
    assert_eq!(f.adapter.total_assets(), 1_000_000);

    // The adapter's own bound never trips the pool's utilization check.
    let vault = Address::generate(&f.env);
    assert_eq!(f.adapter.withdraw(&1_000_000, &vault), max);
    assert_eq!(f.adapter.max_withdraw(), 0);
    assert_eq!(f.adapter.withdraw(&1, &vault), 0);
}

#[test]
fn only_the_owner_moves_the_position() {
    let f = setup();
    f.supply(1_000);
    f.env.set_auths(&[]);
    assert!(f.adapter.try_deposit(&1).is_err());
    assert!(f
        .adapter
        .try_withdraw(&1, &Address::generate(&f.env))
        .is_err());
}

#[test]
#[should_panic]
fn a_pool_without_the_asset_is_refused() {
    let f = setup();
    let other = f
        .env
        .register_stellar_asset_contract_v2(Address::generate(&f.env))
        .address();
    f.env.register(
        BlendAdapter,
        (f.owner.clone(), other, f.pool.address.clone()),
    );
}
