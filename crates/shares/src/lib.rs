#![no_std]
//! Share accounting for the Octarine vault (vault shares) and facility (oLP).
//! Library only: each contract calls these and expands `share_token!` for its
//! SEP-41 interface. Virtual 10^6 shares defend against the first-depositor
//! inflation attack; conversions round in the issuer's favour.

use common::math::{self, MathError, BPS, ONE};
use soroban_sdk::{
    contracterror, contracttype, panic_with_error, symbol_short, Address, Env, String,
};

pub const VIRTUAL: i128 = 1_000_000;
pub const DECIMALS_OFFSET: u32 = 6;
const THRESHOLD: u32 = 518_400;
const EXTEND: u32 = 535_680;

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ShareError {
    InvalidAmount = 100,
    InsufficientBalance = 101,
    InsufficientAllowance = 102,
    NotAllowlisted = 103,
    Frozen = 104,
    Overflow = 105,
}

#[contracttype]
#[derive(Clone)]
enum ShareKey {
    Meta,
    Supply,
    Hwm,
    Allowlist,
    Balance(Address),
    Allowance(Address, Address),
    Member(Address),
    Frozen(Address),
}

#[contracttype]
#[derive(Clone)]
pub struct Meta {
    pub decimals: u32,
    pub name: String,
    pub symbol: String,
}

#[contracttype]
#[derive(Clone)]
struct AllowanceValue {
    amount: i128,
    expiration_ledger: u32,
}

/// The issuer as it stands, with any performance fee due counted as minted.
pub struct Snapshot {
    pub nav: i128,
    pub supply: i128,
    pub fee_shares: i128,
    pub hwm: i128,
}

pub fn md(env: &Env, a: i128, b: i128, d: i128) -> i128 {
    checked(env, math::mul_div(env, a, b, d))
}

pub fn md_up(env: &Env, a: i128, b: i128, d: i128) -> i128 {
    checked(env, math::mul_div_ceil(env, a, b, d))
}

fn checked(env: &Env, r: Result<i128, MathError>) -> i128 {
    match r {
        Ok(v) => v,
        Err(MathError::Domain) => panic_with_error!(env, ShareError::InvalidAmount),
        Err(MathError::Overflow) => panic_with_error!(env, ShareError::Overflow),
    }
}

fn get<T: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>>(env: &Env, k: &ShareKey) -> Option<T> {
    env.storage().persistent().get(k)
}

fn set<T: soroban_sdk::IntoVal<Env, soroban_sdk::Val>>(env: &Env, k: ShareKey, v: &T) {
    env.storage().persistent().set(&k, v);
    env.storage().persistent().extend_ttl(&k, THRESHOLD, EXTEND);
}

pub fn init(env: &Env, asset_decimals: u32, name: String, symbol: String) {
    init_meta(env, asset_decimals + DECIMALS_OFFSET, name, symbol);
    set_hwm(env, share_price(env, 0, 0));
}

/// SEP-41 metadata alone, for a token that is not a vault share.
pub fn init_meta(env: &Env, decimals: u32, name: String, symbol: String) {
    let meta = Meta {
        decimals,
        name,
        symbol,
    };
    env.storage().instance().set(&ShareKey::Meta, &meta);
}

pub fn meta(env: &Env) -> Meta {
    env.storage().instance().get(&ShareKey::Meta).unwrap()
}

pub fn supply(env: &Env) -> i128 {
    env.storage().instance().get(&ShareKey::Supply).unwrap_or(0)
}

fn set_supply(env: &Env, v: i128) {
    env.storage().instance().set(&ShareKey::Supply, &v);
}

pub fn hwm(env: &Env) -> i128 {
    env.storage().instance().get(&ShareKey::Hwm).unwrap_or(0)
}

fn set_hwm(env: &Env, v: i128) {
    env.storage().instance().set(&ShareKey::Hwm, &v);
}

pub fn balance(env: &Env, who: &Address) -> i128 {
    get(env, &ShareKey::Balance(who.clone())).unwrap_or(0)
}

fn set_balance(env: &Env, who: &Address, v: i128) {
    set(env, ShareKey::Balance(who.clone()), &v);
}

pub fn allowance(env: &Env, from: &Address, spender: &Address) -> i128 {
    match get::<AllowanceValue>(env, &ShareKey::Allowance(from.clone(), spender.clone())) {
        Some(a) if a.expiration_ledger >= env.ledger().sequence() => a.amount,
        _ => 0,
    }
}

pub fn approve(env: &Env, from: &Address, spender: &Address, amount: i128, expiration_ledger: u32) {
    if amount < 0 || (amount > 0 && expiration_ledger < env.ledger().sequence()) {
        panic_with_error!(env, ShareError::InvalidAmount);
    }
    let v = AllowanceValue {
        amount,
        expiration_ledger,
    };
    set(env, ShareKey::Allowance(from.clone(), spender.clone()), &v);
    env.events().publish(
        (symbol_short!("approve"), from.clone(), spender.clone()),
        (amount, expiration_ledger),
    );
}

pub fn spend_allowance(env: &Env, from: &Address, spender: &Address, amount: i128) {
    let left = allowance(env, from, spender);
    if left < amount {
        panic_with_error!(env, ShareError::InsufficientAllowance);
    }
    let key = ShareKey::Allowance(from.clone(), spender.clone());
    let mut v: AllowanceValue = get(env, &key).unwrap();
    v.amount = left - amount;
    set(env, key, &v);
}

pub fn allowlist_enabled(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&ShareKey::Allowlist)
        .unwrap_or(false)
}

pub fn set_allowlist(env: &Env, enabled: bool) {
    env.storage().instance().set(&ShareKey::Allowlist, &enabled);
}

pub fn is_member(env: &Env, who: &Address) -> bool {
    get(env, &ShareKey::Member(who.clone())).unwrap_or(false)
}

pub fn set_member(env: &Env, who: &Address, allowed: bool) {
    set(env, ShareKey::Member(who.clone()), &allowed);
    env.events()
        .publish((symbol_short!("member"), who.clone()), allowed);
}

/// With the allowlist on, only members (and the issuer itself) hold shares.
pub fn admitted(env: &Env, who: &Address) -> bool {
    !allowlist_enabled(env) || who == &env.current_contract_address() || is_member(env, who)
}

pub fn is_frozen(env: &Env, who: &Address) -> bool {
    get(env, &ShareKey::Frozen(who.clone())).unwrap_or(false)
}

pub fn set_frozen(env: &Env, who: &Address, frozen: bool) {
    set(env, ShareKey::Frozen(who.clone()), &frozen);
    env.events()
        .publish((symbol_short!("frozen"), who.clone()), frozen);
}

pub fn mint(env: &Env, to: &Address, amount: i128) {
    set_balance(env, to, balance(env, to) + amount);
    set_supply(env, supply(env) + amount);
    if amount != 0 {
        env.events()
            .publish((symbol_short!("mint"), to.clone()), amount);
    }
}

pub fn burn(env: &Env, from: &Address, amount: i128) {
    debit(env, from, amount);
    set_supply(env, supply(env) - amount);
    env.events()
        .publish((symbol_short!("burn"), from.clone()), amount);
}

pub fn transfer(env: &Env, from: &Address, to: &Address, amount: i128) {
    if !admitted(env, to) {
        panic_with_error!(env, ShareError::NotAllowlisted);
    }
    if is_frozen(env, to) {
        panic_with_error!(env, ShareError::Frozen);
    }
    debit(env, from, amount);
    set_balance(env, to, balance(env, to) + amount);
    env.events().publish(
        (symbol_short!("transfer"), from.clone(), to.clone()),
        amount,
    );
}

/// Compliance: moves a frozen holder's shares regardless of the freeze.
pub fn seize(env: &Env, from: &Address, to: &Address, amount: i128) {
    let held = balance(env, from);
    if amount <= 0 || held < amount {
        panic_with_error!(env, ShareError::InsufficientBalance);
    }
    set_balance(env, from, held - amount);
    set_balance(env, to, balance(env, to) + amount);
    env.events()
        .publish((symbol_short!("seize"), from.clone(), to.clone()), amount);
}

fn debit(env: &Env, from: &Address, amount: i128) {
    if amount < 0 {
        panic_with_error!(env, ShareError::InvalidAmount);
    }
    if is_frozen(env, from) {
        panic_with_error!(env, ShareError::Frozen);
    }
    let held = balance(env, from);
    if held < amount {
        panic_with_error!(env, ShareError::InsufficientBalance);
    }
    set_balance(env, from, held - amount);
}

pub fn share_price(env: &Env, nav: i128, supply: i128) -> i128 {
    md(env, nav + 1, ONE, supply + VIRTUAL)
}

/// The performance fee due above the high-water mark, as shares to mint.
pub fn snapshot(env: &Env, nav: i128, fee_bps: u32) -> Snapshot {
    let minted = supply(env);
    let mark = hwm(env);
    let now = share_price(env, nav, minted);
    let mut fee_shares = 0;
    if now > mark && fee_bps > 0 && minted > 0 {
        let gain = md(env, now - mark, minted + VIRTUAL, ONE);
        let fee = md(env, gain, fee_bps as i128, BPS);
        if fee > 0 && fee <= nav {
            fee_shares = md(env, fee, minted + VIRTUAL, nav + 1 - fee);
        }
    }
    let hwm = if now > mark {
        share_price(env, nav, minted + fee_shares)
    } else {
        mark
    };
    Snapshot {
        nav,
        supply: minted + fee_shares,
        fee_shares,
        hwm,
    }
}

/// Mints the fee due to `curator` and moves the high-water mark.
/// Writes the fee and high-water mark unconditionally: venue yield moves NAV
/// between simulation and execution, so a conditional write would fall
/// outside the simulated footprint.
pub fn accrue(env: &Env, nav: i128, fee_bps: u32, curator: &Address) -> Snapshot {
    let s = snapshot(env, nav, fee_bps);
    mint(env, curator, s.fee_shares);
    set_hwm(env, s.hwm);
    s
}

pub fn to_shares(env: &Env, s: &Snapshot, assets: i128, up: bool) -> i128 {
    let f = if up { md_up } else { md };
    f(env, assets, s.supply + VIRTUAL, s.nav + 1)
}

pub fn to_assets(env: &Env, s: &Snapshot, shares: i128, up: bool) -> i128 {
    let f = if up { md_up } else { md };
    f(env, shares, s.nav + 1, s.supply + VIRTUAL)
}

/// The SEP-41 interface over these shares, for contract `$t`.
#[macro_export]
macro_rules! share_token {
    ($t:ident) => {
        #[soroban_sdk::contractimpl]
        impl soroban_sdk::token::TokenInterface for $t {
            fn allowance(env: Env, from: Address, spender: Address) -> i128 {
                $crate::allowance(&env, &from, &spender)
            }
            fn approve(
                env: Env,
                from: Address,
                spender: Address,
                amount: i128,
                expiration_ledger: u32,
            ) {
                from.require_auth();
                $crate::approve(&env, &from, &spender, amount, expiration_ledger);
            }
            fn balance(env: Env, id: Address) -> i128 {
                $crate::balance(&env, &id)
            }
            fn transfer(env: Env, from: Address, to: Address, amount: i128) {
                from.require_auth();
                $crate::transfer(&env, &from, &to, amount);
            }
            fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
                spender.require_auth();
                $crate::spend_allowance(&env, &from, &spender, amount);
                $crate::transfer(&env, &from, &to, amount);
            }
            fn burn(env: Env, from: Address, amount: i128) {
                from.require_auth();
                $crate::burn(&env, &from, amount);
            }
            fn burn_from(env: Env, spender: Address, from: Address, amount: i128) {
                spender.require_auth();
                $crate::spend_allowance(&env, &from, &spender, amount);
                $crate::burn(&env, &from, amount);
            }
            fn decimals(env: Env) -> u32 {
                $crate::meta(&env).decimals
            }
            fn name(env: Env) -> soroban_sdk::String {
                $crate::meta(&env).name
            }
            fn symbol(env: Env) -> soroban_sdk::String {
                $crate::meta(&env).symbol
            }
        }
    };
}
