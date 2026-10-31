#![no_std]
//! Test/testnet yield venue. The position is the adapter's own token balance,
//! so yield is simulated by sending it tokens. An operator (the `venue` slot of
//! the standard adapter constructor) can cap what is withdrawable, to stand in
//! for a lending pool at high utilization. Never deploy to mainnet.

use common::interfaces::VenueAdapter;
use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Env};

#[cfg(test)]
mod test;

#[contracttype]
enum Key {
    Owner,
    Asset,
    Operator,
    Liquidity,
}

#[contract]
pub struct MockVenue;

#[contractimpl]
impl MockVenue {
    pub fn __constructor(env: Env, owner: Address, asset: Address, venue: Address) {
        let s = env.storage().instance();
        s.set(&Key::Owner, &owner);
        s.set(&Key::Asset, &asset);
        s.set(&Key::Operator, &venue);
    }

    /// Cap instant liquidity at `amount`; `None` lifts the cap.
    pub fn set_liquidity(env: Env, amount: Option<i128>) {
        let operator: Address = env.storage().instance().get(&Key::Operator).unwrap();
        operator.require_auth();
        match amount {
            Some(a) => env.storage().instance().set(&Key::Liquidity, &a),
            None => env.storage().instance().remove(&Key::Liquidity),
        }
    }

    fn owner(env: &Env) -> Address {
        env.storage().instance().get(&Key::Owner).unwrap()
    }

    fn asset(env: &Env) -> token::Client<'_> {
        let asset: Address = env.storage().instance().get(&Key::Asset).unwrap();
        token::Client::new(env, &asset)
    }
}

#[contractimpl]
impl VenueAdapter for MockVenue {
    fn deposit(env: Env, amount: i128) -> i128 {
        Self::owner(&env).require_auth();
        amount
    }

    fn withdraw(env: Env, amount: i128, to: Address) -> i128 {
        Self::owner(&env).require_auth();
        let sent = amount.min(Self::max_withdraw(env.clone())).max(0);
        if sent > 0 {
            Self::asset(&env).transfer(&env.current_contract_address(), &to, &sent);
        }
        sent
    }

    fn total_assets(env: Env) -> i128 {
        Self::asset(&env).balance(&env.current_contract_address())
    }

    fn max_withdraw(env: Env) -> i128 {
        let held = Self::total_assets(env.clone());
        match env.storage().instance().get::<_, i128>(&Key::Liquidity) {
            Some(cap) => held.min(cap),
            None => held,
        }
    }
}
