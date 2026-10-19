#![no_std]
//! Octarine vault: holds one asset (a stablecoin or an RWA), issues SEP-56
//! shares against it, and puts idle funds to work in venue adapters. A
//! facility holds vault shares; anyone else may deposit directly. For a
//! permissioned RWA, the vault is the address the issuer allowlists. See
//! docs/FACILITY_V2_STELLAR.md.

mod errors;

#[cfg(test)]
mod test;

use common::interfaces::{VaultHostClient, VenueAdapterClient};
use common::timelock;
use common::types::VaultConfig;
use shares::Snapshot;
use soroban_sdk::token::TokenInterface;
use soroban_sdk::{
    contract, contractimpl, contracttype, panic_with_error, symbol_short, token, Address, BytesN,
    Env, Vec,
};

pub use errors::Error;

const MAX_CURATOR_FEE_BPS: u32 = 5_000;
/// NAV visits every venue on every state change; this keeps it bounded.
const MAX_VENUES: u32 = 8;

#[contracttype]
enum Key {
    Host,
    Config,
    Venues,
}

/// Changes that wait out the timelock. `Venue` deploys an adapter instance
/// from allowlisted code, pointed at the given venue.
#[contracttype]
#[derive(Clone)]
pub enum Change {
    CuratorFee(u32),
    DepositCap(i128),
    Venue(BytesN<32>, Address),
}

#[contract]
pub struct Vault;

shares::share_token!(Vault);

#[contractimpl]
impl Vault {
    pub fn __constructor(env: Env, host: Address, config: VaultConfig) {
        if config.deposit_cap < 0 || config.curator_fee_bps > MAX_CURATOR_FEE_BPS {
            panic_with_error!(&env, Error::InvalidConfig);
        }
        let decimals = token::Client::new(&env, &config.asset).decimals();
        shares::init(&env, decimals, config.name.clone(), config.symbol.clone());
        timelock::init(&env, config.timelock, &config.guardian);
        env.storage().instance().set(&Key::Host, &host);
        env.storage().instance().set(&Key::Config, &config);
    }

    pub fn config(env: Env) -> VaultConfig {
        env.storage().instance().get(&Key::Config).unwrap()
    }

    pub fn query_asset(env: Env) -> Address {
        Self::config(env).asset
    }

    pub fn total_assets(env: Env) -> i128 {
        Self::snapshot(&env).nav
    }

    pub fn total_supply(env: Env) -> i128 {
        Self::snapshot(&env).supply
    }

    pub fn high_water_mark(env: Env) -> i128 {
        shares::hwm(&env)
    }

    /// Adapter instances, in the order exits draw on them.
    pub fn venues(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&Key::Venues)
            .unwrap_or(Vec::new(&env))
    }

    pub fn convert_to_shares(env: Env, assets: i128) -> i128 {
        shares::to_shares(&env, &Self::snapshot(&env), assets, false)
    }

    pub fn convert_to_assets(env: Env, shares: i128) -> i128 {
        shares::to_assets(&env, &Self::snapshot(&env), shares, false)
    }

    pub fn max_deposit(env: Env, receiver: Address) -> i128 {
        let cap = Self::config(env.clone()).deposit_cap;
        match (shares::admitted(&env, &receiver), cap) {
            (false, _) => 0,
            (true, 0) => i128::MAX,
            _ => (cap - Self::snapshot(&env).nav).max(0),
        }
    }

    pub fn max_mint(env: Env, receiver: Address) -> i128 {
        match Self::max_deposit(env.clone(), receiver) {
            i128::MAX => i128::MAX,
            a => shares::to_shares(&env, &Self::snapshot(&env), a, false),
        }
    }

    pub fn max_withdraw(env: Env, owner: Address) -> i128 {
        let s = Self::snapshot(&env);
        shares::to_assets(&env, &s, shares::balance(&env, &owner), false).min(Self::liquidity(&env))
    }

    pub fn max_redeem(env: Env, owner: Address) -> i128 {
        let payable = shares::to_shares(&env, &Self::snapshot(&env), Self::liquidity(&env), false);
        shares::balance(&env, &owner).min(payable)
    }

    pub fn preview_deposit(env: Env, assets: i128) -> i128 {
        shares::to_shares(&env, &Self::snapshot(&env), assets, false)
    }

    pub fn preview_mint(env: Env, shares: i128) -> i128 {
        shares::to_assets(&env, &Self::snapshot(&env), shares, true)
    }

    pub fn preview_withdraw(env: Env, assets: i128) -> i128 {
        shares::to_shares(&env, &Self::snapshot(&env), assets, true)
    }

    pub fn preview_redeem(env: Env, shares: i128) -> i128 {
        shares::to_assets(&env, &Self::snapshot(&env), shares, false)
    }

    pub fn deposit(
        env: Env,
        assets: i128,
        receiver: Address,
        from: Address,
        operator: Address,
    ) -> i128 {
        let s = Self::enter(&env, &from, &receiver, &operator, assets);
        let received = Self::pull(&env, &from, assets);
        let minted = shares::to_shares(&env, &s, received, false);
        Self::credit(&env, &receiver, received, minted)
    }

    pub fn mint(
        env: Env,
        shares: i128,
        receiver: Address,
        from: Address,
        operator: Address,
    ) -> i128 {
        let assets = shares::to_assets(&env, &Self::snapshot(&env), shares, true);
        Self::enter(&env, &from, &receiver, &operator, assets);
        if Self::pull(&env, &from, assets) < assets {
            panic_with_error!(&env, Error::InvalidAmount);
        }
        Self::credit(&env, &receiver, assets, shares);
        assets
    }

    pub fn withdraw(
        env: Env,
        assets: i128,
        receiver: Address,
        owner: Address,
        operator: Address,
    ) -> i128 {
        let s = Self::accrue_now(&env);
        let burned = shares::to_shares(&env, &s, assets, true);
        Self::exit(&env, &owner, &operator, &receiver, burned, assets);
        burned
    }

    pub fn redeem(
        env: Env,
        shares: i128,
        receiver: Address,
        owner: Address,
        operator: Address,
    ) -> i128 {
        let s = Self::accrue_now(&env);
        let assets = shares::to_assets(&env, &s, shares, false);
        Self::exit(&env, &owner, &operator, &receiver, shares, assets);
        assets
    }

    /// Mints any performance fee due and moves the high-water mark.
    pub fn accrue(env: Env) {
        Self::accrue_now(&env);
    }

    pub fn submit(env: Env, change: Change) -> u64 {
        Self::config(env.clone()).curator.require_auth();
        if let Change::CuratorFee(bps) = change {
            if bps > MAX_CURATOR_FEE_BPS {
                panic_with_error!(&env, Error::InvalidConfig);
            }
        }
        timelock::submit(&env, Self::kind(&change), &change)
    }

    /// Anyone may apply a change once its delay has passed.
    pub fn accept(env: Env, kind: u32) {
        let mut c = Self::config(env.clone());
        match timelock::take::<Change>(&env, kind) {
            Change::CuratorFee(bps) => {
                Self::accrue_now(&env);
                c.curator_fee_bps = bps;
            }
            Change::DepositCap(cap) if cap >= 0 => c.deposit_cap = cap,
            Change::Venue(wasm, venue) => Self::add_venue(&env, &c, wasm, venue),
            _ => panic_with_error!(&env, Error::InvalidConfig),
        }
        env.storage().instance().set(&Key::Config, &c);
    }

    pub fn pending(env: Env, kind: u32) -> Option<(Change, u64)> {
        timelock::pending(&env, kind)
    }

    pub fn revoke(env: Env, kind: u32, caller: Address) {
        timelock::revoke(&env, kind, &caller, &Self::config(env.clone()).curator);
    }

    pub fn set_manager(env: Env, manager: Address) {
        let mut c = Self::curator_auth(&env);
        c.manager = manager;
        env.storage().instance().set(&Key::Config, &c);
    }

    pub fn set_allowlist(env: Env, enabled: bool) {
        Self::curator_auth(&env);
        shares::set_allowlist(&env, enabled);
    }

    pub fn set_member(env: Env, who: Address, allowed: bool) {
        Self::curator_auth(&env);
        shares::set_member(&env, &who, allowed);
    }

    pub fn freeze(env: Env, who: Address, frozen: bool) {
        Self::manager_auth(&env);
        shares::set_frozen(&env, &who, frozen);
    }

    pub fn seize(env: Env, from: Address, to: Address, amount: i128) {
        Self::manager_auth(&env);
        shares::seize(&env, &from, &to, amount);
    }

    /// Only an empty venue can be dropped, so no position is ever orphaned.
    pub fn disable_venue(env: Env, adapter: Address) {
        Self::curator_auth(&env);
        let mut venues = Self::venues(env.clone());
        let i = Self::venue_index(&env, &venues, &adapter);
        if VenueAdapterClient::new(&env, &adapter).total_assets() > 0 {
            panic_with_error!(&env, Error::VenueNotEmpty);
        }
        venues.remove(i);
        env.storage().instance().set(&Key::Venues, &venues);
    }

    /// `order` must name exactly the current set.
    pub fn set_venue_order(env: Env, order: Vec<Address>) {
        Self::manager_auth(&env);
        let venues = Self::venues(env.clone());
        for a in order.iter() {
            Self::venue_index(&env, &venues, &a);
            if order.first_index_of(&a) != order.last_index_of(&a) {
                panic_with_error!(&env, Error::InvalidConfig);
            }
        }
        if order.len() != venues.len() {
            panic_with_error!(&env, Error::InvalidConfig);
        }
        env.storage().instance().set(&Key::Venues, &order);
    }

    pub fn allocate(env: Env, adapter: Address, amount: i128) {
        Self::manager_auth(&env);
        Self::venue_index(&env, &Self::venues(env.clone()), &adapter);
        if amount <= 0 || amount > Self::idle(&env) {
            panic_with_error!(&env, Error::InvalidAmount);
        }
        Self::accrue_now(&env);
        Self::send(&env, &adapter, amount);
        VenueAdapterClient::new(&env, &adapter).deposit(&amount);
        env.events()
            .publish((symbol_short!("allocate"), adapter), amount);
    }

    /// Returns what actually came back, measured here.
    pub fn deallocate(env: Env, adapter: Address, amount: i128) -> i128 {
        Self::manager_auth(&env);
        Self::venue_index(&env, &Self::venues(env.clone()), &adapter);
        Self::accrue_now(&env);
        let before = Self::idle(&env);
        VenueAdapterClient::new(&env, &adapter).withdraw(&amount, &env.current_contract_address());
        let returned = Self::idle(&env) - before;
        env.events()
            .publish((symbol_short!("dealloc"), adapter), returned);
        returned
    }

    /// Protocol admin only, and only to the code the host has registered.
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        let host = VaultHostClient::new(&env, &Self::host(&env));
        host.admin().require_auth();
        if new_wasm_hash != host.vault_wasm() {
            panic_with_error!(&env, Error::InvalidConfig);
        }
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    fn kind(c: &Change) -> u32 {
        match c {
            Change::CuratorFee(_) => 0,
            Change::DepositCap(_) => 1,
            Change::Venue(..) => 2,
        }
    }

    fn add_venue(env: &Env, c: &VaultConfig, wasm: BytesN<32>, venue: Address) {
        let mut venues = Self::venues(env.clone());
        if venues.len() >= MAX_VENUES {
            panic_with_error!(env, Error::TooManyVenues);
        }
        let me = env.current_contract_address();
        let adapter = VaultHostClient::new(env, &Self::host(env))
            .deploy_adapter(&me, &wasm, &c.asset, &venue);
        venues.push_back(adapter.clone());
        env.storage().instance().set(&Key::Venues, &venues);
        env.events()
            .publish((symbol_short!("venue"), adapter), venue);
    }

    fn host(env: &Env) -> Address {
        env.storage().instance().get(&Key::Host).unwrap()
    }

    fn curator_auth(env: &Env) -> VaultConfig {
        let c = Self::config(env.clone());
        c.curator.require_auth();
        c
    }

    fn manager_auth(env: &Env) -> VaultConfig {
        let c = Self::config(env.clone());
        c.manager.require_auth();
        c
    }

    fn venue_index(env: &Env, venues: &Vec<Address>, adapter: &Address) -> u32 {
        match venues.first_index_of(adapter) {
            Some(i) => i,
            None => panic_with_error!(env, Error::VenueNotFound),
        }
    }

    fn idle(env: &Env) -> i128 {
        token::Client::new(env, &Self::config(env.clone()).asset)
            .balance(&env.current_contract_address())
    }

    fn gross(env: &Env) -> i128 {
        let mut total = Self::idle(env);
        for v in Self::venues(env.clone()).iter() {
            total += VenueAdapterClient::new(env, &v).total_assets();
        }
        total
    }

    /// Payable right now: idle plus what venues can return in this transaction.
    fn liquidity(env: &Env) -> i128 {
        let mut total = Self::idle(env);
        for v in Self::venues(env.clone()).iter() {
            total += VenueAdapterClient::new(env, &v).max_withdraw();
        }
        total
    }

    fn snapshot(env: &Env) -> Snapshot {
        shares::snapshot(
            env,
            Self::gross(env),
            Self::config(env.clone()).curator_fee_bps,
        )
    }

    fn accrue_now(env: &Env) -> Snapshot {
        let c = Self::config(env.clone());
        env.storage().instance().extend_ttl(518_400, 535_680);
        shares::accrue(env, Self::gross(env), c.curator_fee_bps, &c.curator)
    }

    /// The payer always consents; an operator acting for them must too.
    fn enter(
        env: &Env,
        from: &Address,
        receiver: &Address,
        operator: &Address,
        assets: i128,
    ) -> Snapshot {
        operator.require_auth();
        if operator != from {
            from.require_auth();
        }
        if assets <= 0 {
            panic_with_error!(env, Error::InvalidAmount);
        }
        if !shares::admitted(env, from) || !shares::admitted(env, receiver) {
            panic_with_error!(env, Error::NotAllowlisted);
        }
        let s = Self::accrue_now(env);
        let cap = Self::config(env.clone()).deposit_cap;
        if cap > 0 && s.nav + assets > cap {
            panic_with_error!(env, Error::DepositCapExceeded);
        }
        s
    }

    fn credit(env: &Env, receiver: &Address, assets: i128, minted: i128) -> i128 {
        if minted <= 0 {
            panic_with_error!(env, Error::InvalidAmount);
        }
        shares::mint(env, receiver, minted);
        env.events().publish(
            (symbol_short!("deposit"), receiver.clone()),
            (assets, minted),
        );
        minted
    }

    /// Credits what actually arrived, so a transfer-taxed asset cannot over-mint.
    fn pull(env: &Env, from: &Address, assets: i128) -> i128 {
        let before = Self::idle(env);
        token::Client::new(env, &Self::config(env.clone()).asset).transfer(
            from,
            &env.current_contract_address(),
            &assets,
        );
        Self::idle(env) - before
    }

    /// An operator other than the owner spends the owner's share allowance.
    fn exit(
        env: &Env,
        owner: &Address,
        operator: &Address,
        receiver: &Address,
        burned: i128,
        assets: i128,
    ) {
        operator.require_auth();
        if burned <= 0 || assets <= 0 {
            panic_with_error!(env, Error::InvalidAmount);
        }
        if operator != owner {
            shares::spend_allowance(env, owner, operator, burned);
        }
        shares::burn(env, owner, burned);
        Self::send(env, receiver, assets);
        env.events().publish(
            (symbol_short!("withdraw"), owner.clone(), receiver.clone()),
            (assets, burned),
        );
    }

    /// Pulls from venues in order when idle is short. Judged by the idle
    /// balance reached, so a venue that returns short makes the caller revert.
    fn send(env: &Env, to: &Address, assets: i128) {
        let me = env.current_contract_address();
        for v in Self::venues(env.clone()).iter() {
            let short = assets - Self::idle(env);
            if short <= 0 {
                break;
            }
            let adapter = VenueAdapterClient::new(env, &v);
            let take = short.min(adapter.max_withdraw());
            if take > 0 {
                adapter.withdraw(&take, &me);
            }
        }
        if Self::idle(env) < assets {
            panic_with_error!(env, Error::InsufficientLiquidity);
        }
        token::Client::new(env, &Self::config(env.clone()).asset).transfer(&me, to, &assets);
    }
}
