#![no_std]
//! Withdrawal queue for one facility. oLP the facility cannot pay out now
//! waits here and is served first come, first served, at the NAV when served.
//! A head position that can only be part-filled keeps its place. The facility
//! reserves the queue's claim against every other exit. See
//! docs/FACILITY_V2_STELLAR.md.

#[cfg(test)]
mod test;

pub use common::types::QueueConfig;
use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, panic_with_error,
    symbol_short, token, Address, Env,
};

/// The facility functions the queue uses.
#[contractclient(name = "FacilityClient")]
pub trait FacilityFns {
    fn convert_to_assets(env: Env, shares: i128) -> i128;
    fn convert_to_shares(env: Env, assets: i128) -> i128;
    fn max_withdraw(env: Env, owner: Address) -> i128;
    fn redeem(env: Env, shares: i128, receiver: Address, owner: Address, operator: Address)
        -> i128;
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    InvalidAmount = 1,
    NotFound = 2,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Position {
    pub owner: Address,
    pub receiver: Address,
    /// Shares still waiting; they keep their exposure until served.
    pub shares: i128,
}

#[contracttype]
enum Key {
    Config,
    Head,
    Tail,
    Pos(u64),
}

#[contract]
pub struct Queue;

#[contractimpl]
impl Queue {
    pub fn __constructor(env: Env, config: QueueConfig) {
        env.storage().instance().set(&Key::Config, &config);
    }

    pub fn config(env: Env) -> QueueConfig {
        env.storage().instance().get(&Key::Config).unwrap()
    }

    pub fn position(env: Env, id: u64) -> Option<Position> {
        env.storage().persistent().get(&Key::Pos(id))
    }

    /// Next position to serve, and the next id to hand out. Equal when empty.
    pub fn bounds(env: Env) -> (u64, u64) {
        let s = env.storage().instance();
        (
            s.get(&Key::Head).unwrap_or(0),
            s.get(&Key::Tail).unwrap_or(0),
        )
    }

    /// Moves `owner`'s oLP into the queue; it redeems to `receiver` when served.
    pub fn request(env: Env, owner: Address, receiver: Address, shares: i128) -> u64 {
        owner.require_auth();
        if shares <= 0 {
            panic_with_error!(&env, Error::InvalidAmount);
        }
        let facility = Self::config(env.clone()).facility;
        token::Client::new(&env, &facility).transfer(
            &owner,
            &env.current_contract_address(),
            &shares,
        );
        let (head, tail) = Self::bounds(env.clone());
        Self::save(
            &env,
            tail,
            &Position {
                owner: owner.clone(),
                receiver,
                shares,
            },
        );
        Self::set_bounds(&env, head, tail + 1);
        env.events()
            .publish((symbol_short!("queued"), tail, owner), shares);
        tail
    }

    /// Serves up to `max` positions from the head with what the facility can
    /// pay now. Anyone may call it.
    pub fn process(env: Env, max: u32) -> u32 {
        let fac = FacilityClient::new(&env, &Self::config(env.clone()).facility);
        let me = env.current_contract_address();
        let (mut head, tail) = Self::bounds(env.clone());
        let mut available = fac.max_withdraw(&me);
        let mut served = 0;
        while head < tail && served < max {
            let Some(mut p) = Self::position(env.clone(), head) else {
                head += 1;
                continue;
            };
            if available <= 0 {
                break;
            }
            let shares = if fac.convert_to_assets(&p.shares) <= available {
                p.shares
            } else {
                fac.convert_to_shares(&available).min(p.shares)
            };
            if shares == 0 {
                break;
            }
            available -= fac.redeem(&shares, &p.receiver, &me, &me);
            p.shares -= shares;
            served += 1;
            env.events()
                .publish((symbol_short!("served"), head), shares);
            if p.shares > 0 {
                Self::save(&env, head, &p);
                break;
            }
            env.storage().persistent().remove(&Key::Pos(head));
            head += 1;
        }
        Self::set_bounds(&env, head, tail);
        served
    }

    /// Returns a position's unserved shares to its owner.
    pub fn cancel(env: Env, id: u64) {
        let p = Self::position(env.clone(), id)
            .unwrap_or_else(|| panic_with_error!(&env, Error::NotFound));
        p.owner.require_auth();
        let facility = Self::config(env.clone()).facility;
        token::Client::new(&env, &facility).transfer(
            &env.current_contract_address(),
            &p.owner,
            &p.shares,
        );
        env.storage().persistent().remove(&Key::Pos(id));
        env.events()
            .publish((symbol_short!("qcancel"), id), p.shares);
    }

    fn save(env: &Env, id: u64, p: &Position) {
        env.storage().persistent().set(&Key::Pos(id), p);
        env.storage()
            .persistent()
            .extend_ttl(&Key::Pos(id), 518_400, 535_680);
    }

    fn set_bounds(env: &Env, head: u64, tail: u64) {
        env.storage().instance().set(&Key::Head, &head);
        env.storage().instance().set(&Key::Tail, &tail);
        env.storage().instance().extend_ttl(518_400, 535_680);
    }
}
