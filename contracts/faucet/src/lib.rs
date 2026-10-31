#![no_std]
//! Testnet faucet. Holds a pre-minted stock of each test token and hands out
//! a fixed drip of every listed token per claim, at most once per cooldown
//! per address. A token whose stock is below its drip is skipped, so one empty
//! token never blocks the rest. Never deploy to mainnet.

#[cfg(test)]
mod test;

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, token,
    vec, Address, Env, Vec,
};

/// ~30 days of ledgers; extended when under ~29 days.
const THRESHOLD: u32 = 518_400;
const EXTEND: u32 = 535_680;

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    Cooldown = 1,
    InvalidAmount = 2,
}

#[contracttype]
enum Key {
    Admin,
    Cooldown,
    /// Ordered `(token, drip)` list.
    Drips,
    LastClaim(Address),
}

#[contract]
pub struct Faucet;

#[contractimpl]
impl Faucet {
    pub fn __constructor(env: Env, admin: Address, cooldown: u64) {
        let s = env.storage().instance();
        s.set(&Key::Admin, &admin);
        s.set(&Key::Cooldown, &cooldown);
        s.set(&Key::Drips, &Vec::<(Address, i128)>::new(&env));
        bump(&env);
    }

    /// Set `token`'s drip per claim; 0 removes it. Admin only.
    pub fn set_drip(env: Env, token: Address, amount: i128) {
        Self::admin(env.clone()).require_auth();
        if amount < 0 {
            panic_with_error!(&env, Error::InvalidAmount);
        }
        let mut drips = Self::drips(env.clone());
        let at = drips.iter().position(|(t, _)| t == token);
        match (at, amount) {
            (Some(i), 0) => {
                drips.remove(i as u32);
            }
            (Some(i), a) => drips.set(i as u32, (token, a)),
            (None, 0) => {}
            (None, a) => drips.push_back((token, a)),
        }
        env.storage().instance().set(&Key::Drips, &drips);
        bump(&env);
    }

    /// Seconds between claims per address. Admin only.
    pub fn set_cooldown(env: Env, cooldown: u64) {
        Self::admin(env.clone()).require_auth();
        env.storage().instance().set(&Key::Cooldown, &cooldown);
        bump(&env);
    }

    /// Send `to` one drip of every token in stock. Returns what was sent.
    pub fn claim(env: Env, to: Address) -> Vec<(Address, i128)> {
        to.require_auth();
        if Self::time_until_next_claim(env.clone(), to.clone()) > 0 {
            panic_with_error!(&env, Error::Cooldown);
        }
        let me = env.current_contract_address();
        let mut sent = vec![&env];
        for (t, drip) in Self::drips(env.clone()).iter() {
            let tok = token::Client::new(&env, &t);
            if tok.balance(&me) >= drip {
                tok.transfer(&me, &to, &drip);
                sent.push_back((t, drip));
            }
        }
        let key = Key::LastClaim(to.clone());
        let p = env.storage().persistent();
        p.set(&key, &env.ledger().timestamp());
        p.extend_ttl(&key, THRESHOLD, EXTEND);
        bump(&env);
        env.events()
            .publish((symbol_short!("claim"), to), sent.clone());
        sent
    }

    /// Seconds until `who` may claim again; 0 if now.
    pub fn time_until_next_claim(env: Env, who: Address) -> u64 {
        let last: Option<u64> = env.storage().persistent().get(&Key::LastClaim(who));
        match last {
            Some(t) => t
                .saturating_add(Self::cooldown(env.clone()))
                .saturating_sub(env.ledger().timestamp()),
            None => 0,
        }
    }

    pub fn drips(env: Env) -> Vec<(Address, i128)> {
        env.storage().instance().get(&Key::Drips).unwrap()
    }

    pub fn cooldown(env: Env) -> u64 {
        env.storage().instance().get(&Key::Cooldown).unwrap()
    }

    pub fn admin(env: Env) -> Address {
        env.storage().instance().get(&Key::Admin).unwrap()
    }
}

fn bump(env: &Env) {
    env.storage().instance().extend_ttl(THRESHOLD, EXTEND);
}
