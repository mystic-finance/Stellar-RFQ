//! Vault-of-vaults: an Octarine vault allocates through this adapter into
//! another Octarine vault. Allocation runs under real auth, so the target's
//! pull succeeds only if the adapter authorised exactly that transfer.

use super::*;
use common::interfaces::VaultHost;
use common::types::VaultConfig;
use soroban_sdk::testutils::{Address as _, Ledger as _, MockAuth, MockAuthInvoke};
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::{BytesN, String};
use vault::{Vault, VaultClient};

/// Stands in for the facility aggregator; `deploy_adapter` hands back the
/// adapter the test registered, passed as `venue`.
#[contract]
pub struct MockHost;

#[contractimpl]
impl VaultHost for MockHost {
    fn admin(env: Env) -> Address {
        env.current_contract_address()
    }
    fn vault_wasm(env: Env) -> BytesN<32> {
        BytesN::from_array(&env, &[9u8; 32])
    }
    fn deploy_adapter(
        _env: Env,
        _vault: Address,
        _hash: BytesN<32>,
        _asset: Address,
        venue: Address,
    ) -> Address {
        venue
    }
}

struct Fixture {
    env: Env,
    outer: VaultClient<'static>,
    inner: VaultClient<'static>,
    adapter: Address,
    manager: Address,
    asset: Address,
}

fn vault(env: &Env, host: &Address, asset: &Address, manager: &Address) -> VaultClient<'static> {
    let config = VaultConfig {
        asset: asset.clone(),
        curator: manager.clone(),
        manager: manager.clone(),
        guardian: manager.clone(),
        name: String::from_str(env, "Octarine"),
        symbol: String::from_str(env, "oct"),
        deposit_cap: 0,
        curator_fee_bps: 0,
        timelock: 0,
    };
    let id = env.register(Vault, (host.clone(), config));
    VaultClient::new(env, &id)
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    env.cost_estimate().budget().reset_unlimited();
    env.ledger().set_timestamp(1_000_000);
    let asset = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let host = env.register(MockHost, ());
    let manager = Address::generate(&env);
    let outer = vault(&env, &host, &asset, &manager);
    let inner = vault(&env, &host, &asset, &Address::generate(&env));
    let adapter = env.register(
        Sep56Adapter,
        (outer.address.clone(), asset.clone(), inner.address.clone()),
    );
    outer.submit(&vault::Change::Venue(
        BytesN::from_array(&env, &[1u8; 32]),
        adapter.clone(),
    ));
    outer.accept(&2);

    let alice = Address::generate(&env);
    StellarAssetClient::new(&env, &asset).mint(&alice, &1_000_000);
    outer.deposit(&1_000_000, &alice, &alice, &alice);
    Fixture {
        env,
        outer,
        inner,
        adapter,
        manager,
        asset,
    }
}

impl Fixture {
    fn adapter(&self) -> Sep56AdapterClient<'_> {
        Sep56AdapterClient::new(&self.env, &self.adapter)
    }

    /// Only the manager's signature is mocked.
    fn allocate(&self, amount: i128) {
        self.env.mock_auths(&[MockAuth {
            address: &self.manager,
            invoke: &MockAuthInvoke {
                contract: &self.outer.address,
                fn_name: "allocate",
                args: (self.adapter.clone(), amount).into_val(&self.env),
                sub_invokes: &[],
            },
        }]);
        self.outer.allocate(&self.adapter, &amount);
        self.env.mock_all_auths();
    }

    fn balance(&self, who: &Address) -> i128 {
        TokenClient::new(&self.env, &self.asset).balance(who)
    }
}

#[test]
fn a_vault_allocates_into_another_vault_under_real_auth() {
    let f = setup();
    f.allocate(600_000);
    assert_eq!(f.balance(&f.inner.address), 600_000);
    assert_eq!(f.balance(&f.outer.address), 400_000);
    assert_eq!(f.balance(&f.adapter), 0);
    assert!(f.inner.balance(&f.adapter) > 0);
    // Virtual shares cost the position at most one unit of rounding.
    assert!((599_999..=600_000).contains(&f.adapter().total_assets()));
    assert!((999_999..=1_000_000).contains(&f.outer.total_assets()));
}

#[test]
fn yield_in_the_target_vault_lifts_the_owners_nav_and_comes_home() {
    let f = setup();
    f.allocate(600_000);
    StellarAssetClient::new(&f.env, &f.asset).mint(&f.inner.address, &60_000);
    let nav = f.outer.total_assets();
    assert!((1_059_998..=1_060_000).contains(&nav));

    let back = f.outer.deallocate(&f.adapter, &i128::MAX);
    assert!((659_998..=660_000).contains(&back));
    assert_eq!(f.balance(&f.outer.address), 400_000 + back);
    assert_eq!(f.adapter().total_assets(), 0);
}

#[test]
fn withdrawals_are_capped_at_what_the_target_can_pay() {
    let f = setup();
    f.allocate(600_000);
    let adapter = f.adapter();
    assert_eq!(adapter.max_withdraw(), f.inner.max_withdraw(&f.adapter));
    let to = Address::generate(&f.env);
    let sent = adapter.withdraw(&1_000_000_000, &to);
    assert_eq!(sent, f.balance(&to));
    assert!((599_999..=600_000).contains(&sent));
    assert_eq!(adapter.withdraw(&1, &to), 0);
}

#[test]
fn only_the_owner_moves_the_position() {
    let f = setup();
    f.allocate(600_000);
    f.env.set_auths(&[]);
    let adapter = f.adapter();
    assert!(adapter.try_deposit(&1).is_err());
    assert!(adapter
        .try_withdraw(&1, &Address::generate(&f.env))
        .is_err());
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn the_target_must_hold_the_owners_asset() {
    let f = setup();
    let other = f
        .env
        .register_stellar_asset_contract_v2(Address::generate(&f.env))
        .address();
    f.env.register(
        Sep56Adapter,
        (f.outer.address.clone(), other, f.inner.address.clone()),
    );
}
