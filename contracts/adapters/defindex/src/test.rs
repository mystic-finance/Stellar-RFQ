//! Against a vault that behaves as DeFindex 1.0.0 does for one asset:
//! `deposit` takes per-asset vectors, authorises `from` and pulls with a nested
//! `transfer`; the first deposit locks 1000 shares in the vault; exits are by
//! shares, paid pro rata rounding down; a strategy that cannot pay reverts the
//! exit. Only the owner's signature is mocked.

use super::*;
use soroban_sdk::contracttype;
use soroban_sdk::testutils::{Address as _, MockAuth, MockAuthInvoke};
use soroban_sdk::token::{StellarAssetClient, TokenClient};

const MINIMUM_LIQUIDITY: i128 = 1000;

#[contracttype]
enum MockKey {
    Asset,
    Supply,
    Frozen,
    Shares(Address),
}

#[contract]
pub struct MockDefindex;

#[contractimpl]
impl MockDefindex {
    pub fn __constructor(env: Env, asset: Address) {
        env.storage().instance().set(&MockKey::Asset, &asset);
    }

    /// A strategy that cannot pay: every exit reverts.
    pub fn freeze(env: Env, frozen: bool) {
        env.storage().instance().set(&MockKey::Frozen, &frozen);
    }

    pub fn deposit(
        env: Env,
        amounts_desired: Vec<i128>,
        amounts_min: Vec<i128>,
        from: Address,
        invest: bool,
    ) -> (Vec<i128>, i128, Option<Vec<Option<i128>>>) {
        from.require_auth();
        assert!(
            amounts_desired.len() == 1 && amounts_min.len() == 1,
            "WrongAmountsLength"
        );
        let _ = invest;
        let amount = amounts_desired.get(0).unwrap();
        let (supply, managed) = (Self::total_supply(env.clone()), Self::managed(&env));
        Self::token(&env).transfer(&from, &env.current_contract_address(), &amount);
        let shares = if supply == 0 {
            assert!(amount > MINIMUM_LIQUIDITY, "InsufficientAmount");
            Self::credit(&env, &env.current_contract_address(), MINIMUM_LIQUIDITY);
            amount - MINIMUM_LIQUIDITY
        } else {
            amount * supply / managed
        };
        Self::credit(&env, &from, shares);
        (vec![&env, amount], shares, None)
    }

    pub fn withdraw(
        env: Env,
        withdraw_shares: i128,
        min_amounts_out: Vec<i128>,
        from: Address,
    ) -> Vec<i128> {
        from.require_auth();
        assert!(
            !env.storage()
                .instance()
                .get(&MockKey::Frozen)
                .unwrap_or(false),
            "StrategyWithdrawError"
        );
        assert!(withdraw_shares > 0, "AmountNotAllowed");
        let out = Self::get_asset_amounts_per_shares(env.clone(), withdraw_shares)
            .get(0)
            .unwrap();
        assert!(
            out >= min_amounts_out.get(0).unwrap(),
            "InsufficientOutputAmount"
        );
        assert!(
            Self::balance(env.clone(), from.clone()) >= withdraw_shares,
            "InsufficientBalance"
        );
        Self::credit(&env, &from, -withdraw_shares);
        Self::token(&env).transfer(&env.current_contract_address(), &from, &out);
        vec![&env, out]
    }

    pub fn get_asset_amounts_per_shares(env: Env, vault_shares: i128) -> Vec<i128> {
        let supply = Self::total_supply(env.clone());
        assert!(
            supply > 0 && vault_shares <= supply,
            "AmountOverTotalSupply"
        );
        vec![&env, Self::managed(&env) * vault_shares / supply]
    }

    pub fn total_supply(env: Env) -> i128 {
        env.storage().instance().get(&MockKey::Supply).unwrap_or(0)
    }

    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&MockKey::Shares(id))
            .unwrap_or(0)
    }

    fn credit(env: &Env, who: &Address, shares: i128) {
        let b = Self::balance(env.clone(), who.clone()) + shares;
        env.storage()
            .persistent()
            .set(&MockKey::Shares(who.clone()), &b);
        let s = Self::total_supply(env.clone()) + shares;
        env.storage().instance().set(&MockKey::Supply, &s);
    }

    fn managed(env: &Env) -> i128 {
        Self::token(env).balance(&env.current_contract_address())
    }

    fn token(env: &Env) -> TokenClient<'_> {
        let a: Address = env.storage().instance().get(&MockKey::Asset).unwrap();
        TokenClient::new(env, &a)
    }
}

struct Fixture {
    env: Env,
    adapter: DefindexAdapterClient<'static>,
    vault: MockDefindexClient<'static>,
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
    let vault = env.register(MockDefindex, (usdc.clone(),));
    let owner = Address::generate(&env);
    let adapter = env.register(
        DefindexAdapter,
        (owner.clone(), usdc.clone(), vault.clone()),
    );
    Fixture {
        adapter: DefindexAdapterClient::new(&env, &adapter),
        vault: MockDefindexClient::new(&env, &vault),
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
fn deposits_under_real_nested_auth_and_tracks_yield() {
    let f = setup();
    f.supply(1_000_000);
    assert_eq!(f.bal(&f.vault.address), 1_000_000);
    assert_eq!(f.bal(&f.adapter.address), 0);
    // The vault's locked minimum liquidity costs the first depositor its share.
    assert_eq!(f.adapter.total_assets(), 999_000);

    StellarAssetClient::new(&f.env, &f.usdc).mint(&f.vault.address, &100_000);
    assert_eq!(f.adapter.total_assets(), 1_098_900);
    assert_eq!(f.adapter.max_withdraw(), f.adapter.total_assets());
}

#[test]
fn an_asset_amount_becomes_enough_shares_and_everything_received_is_forwarded() {
    let f = setup();
    f.supply(1_000_000);
    StellarAssetClient::new(&f.env, &f.usdc).mint(&f.vault.address, &37_777);
    let to = Address::generate(&f.env);
    let sent = f.adapter.withdraw(&300_000, &to);
    assert!((300_000..=300_002).contains(&sent), "sent {sent}");
    assert_eq!(f.bal(&to), sent);
    assert_eq!(f.bal(&f.adapter.address), 0);

    // More than the position redeems every share, no more.
    let rest = f.adapter.withdraw(&i128::MAX, &to);
    assert_eq!(f.vault.balance(&f.adapter.address), 0);
    assert_eq!(f.bal(&to), sent + rest);
    assert_eq!(f.adapter.total_assets(), 0);
    assert_eq!(f.adapter.withdraw(&1, &to), 0);
}

#[test]
fn a_strategy_that_cannot_pay_reverts_the_exit() {
    let f = setup();
    f.supply(1_000_000);
    f.vault.freeze(&true);
    assert!(f
        .adapter
        .try_withdraw(&1_000, &Address::generate(&f.env))
        .is_err());
}

#[test]
fn only_the_owner_moves_the_position() {
    let f = setup();
    f.supply(10_000);
    f.env.set_auths(&[]);
    assert!(f.adapter.try_deposit(&1).is_err());
    assert!(f
        .adapter
        .try_withdraw(&1, &Address::generate(&f.env))
        .is_err());
}
