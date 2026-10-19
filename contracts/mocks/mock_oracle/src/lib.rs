#![no_std]
//! Test/testnet price feed exposing the `oracle` adapter's `get_price`. An
//! operator sets the price; a reading is stamped now unless a stamp is given.
//! Price 0 means no price. Never deploy to mainnet.

use common::interfaces::Oracle;
use common::types::PriceData;
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

#[contracttype]
enum Key {
    Operator,
    Price,
    Stamp,
}

#[contract]
pub struct MockOracle;

#[contractimpl]
impl MockOracle {
    pub fn __constructor(env: Env, operator: Address, price: i128) {
        env.storage().instance().set(&Key::Operator, &operator);
        env.storage().instance().set(&Key::Price, &price);
    }

    pub fn set_price(env: Env, price: i128, timestamp: Option<u64>) {
        let operator: Address = env.storage().instance().get(&Key::Operator).unwrap();
        operator.require_auth();
        env.storage().instance().set(&Key::Price, &price);
        match timestamp {
            Some(t) => env.storage().instance().set(&Key::Stamp, &t),
            None => env.storage().instance().remove(&Key::Stamp),
        }
    }
}

#[contractimpl]
impl Oracle for MockOracle {
    fn get_price(env: Env, _base: Address, _quote: Address) -> PriceData {
        let s = env.storage().instance();
        PriceData {
            price: s.get(&Key::Price).unwrap(),
            timestamp: s.get(&Key::Stamp).unwrap_or(env.ledger().timestamp()),
        }
    }
}
