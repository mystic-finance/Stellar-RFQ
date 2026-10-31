#![no_std]
//! Octarine facility aggregator: deploys vaults, facilities and queues from
//! pinned code and venue adapters from allowlisted code; lists the facilities
//! that may trade; and routes trades to them, standalone (`swap`) or as the
//! RFQ router's Facility source (`fill`). It takes the protocol fee in the
//! base asset: from the output when selling RWA, from the input when buying,
//! so it never holds an RWA. See docs/FACILITY_V2_STELLAR.md.

mod errors;

#[cfg(test)]
mod test;

use common::interfaces::{Aggregator, VaultHost};
use common::math::{self, BPS};
use common::types::{FacilityConfig, FacilityRoute, QueueConfig, VaultConfig};
use soroban_sdk::xdr::FromXdr;
use soroban_sdk::{
    contract, contractclient, contractimpl, contracttype, panic_with_error, symbol_short, token,
    Address, Bytes, BytesN, Env, IntoVal, Val, Vec,
};

pub use errors::Error;

const MAX_FEE_BPS: u32 = 500;
/// `quote_all` visits every active facility; this keeps it inside one simulation.
const MAX_ACTIVE: u32 = 20;

/// The facility functions the aggregator uses.
#[contractclient(name = "FacilityClient")]
pub trait FacilityFns {
    fn base(env: Env) -> Address;
    fn base_vault(env: Env) -> Address;
    fn quote(env: Env, token_in: Address, token_out: Address, amount_in: i128) -> i128;
    fn trade(
        env: Env,
        payer: Address,
        recipient: Address,
        token_in: Address,
        token_out: Address,
        amount_in: i128,
        min_out: i128,
    ) -> i128;
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Wasms {
    pub vault: BytesN<32>,
    pub facility: BytesN<32>,
    pub queue: BytesN<32>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FacilityQuote {
    pub facility: Address,
    /// What the trader receives, net of the protocol fee.
    pub amount_out: i128,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Vault,
    Facility,
    Queue,
}

#[contracttype]
enum Key {
    Admin,
    Router,
    Wasms,
    Fee,
    Paused,
    Active,
    Salt,
    Deployed(Address),
    Adapter(BytesN<32>),
}

#[contract]
pub struct FacilityAggregator;

#[contractimpl]
impl FacilityAggregator {
    pub fn __constructor(
        env: Env,
        admin: Address,
        router: Address,
        wasms: Wasms,
        fee_recipient: Address,
        fee_bps: u32,
    ) {
        Self::check_fee(&env, fee_bps);
        let s = env.storage().instance();
        s.set(&Key::Admin, &admin);
        s.set(&Key::Router, &router);
        s.set(&Key::Wasms, &wasms);
        s.set(&Key::Fee, &(fee_recipient, fee_bps));
        s.set(&Key::Active, &Vec::<Address>::new(&env));
    }

    /// Anyone may deploy, as curator; trading is a separate, admin-gated step.
    pub fn create_vault(env: Env, config: VaultConfig) -> Address {
        config.curator.require_auth();
        let wasm = Self::wasms(env.clone()).vault;
        Self::deploy(
            &env,
            Kind::Vault,
            wasm,
            (env.current_contract_address(), config).into_val(&env),
        )
    }

    /// Its base vault must be one of ours.
    pub fn create_facility(env: Env, config: FacilityConfig) -> Address {
        config.curator.require_auth();
        Self::expect(&env, &config.base_vault, Kind::Vault);
        let wasm = Self::wasms(env.clone()).facility;
        Self::deploy(
            &env,
            Kind::Facility,
            wasm,
            (env.current_contract_address(), config).into_val(&env),
        )
    }

    /// For one of our facilities.
    pub fn create_queue(env: Env, config: QueueConfig) -> Address {
        config.curator.require_auth();
        Self::expect(&env, &config.facility, Kind::Facility);
        let wasm = Self::wasms(env.clone()).queue;
        Self::deploy(&env, Kind::Queue, wasm, (config,).into_val(&env))
    }

    pub fn set_active(env: Env, facility: Address, active: bool) {
        Self::admin_auth(&env);
        Self::expect(&env, &facility, Kind::Facility);
        let mut list = Self::active(env.clone());
        match (list.first_index_of(&facility), active) {
            (None, true) if list.len() >= MAX_ACTIVE => {
                panic_with_error!(&env, Error::TooManyActive)
            }
            (None, true) => list.push_back(facility.clone()),
            (Some(i), false) => {
                list.remove(i);
            }
            _ => return,
        }
        env.storage().instance().set(&Key::Active, &list);
        env.events()
            .publish((symbol_short!("active"), facility), active);
    }

    /// The protocol vets adapter code once; curators choose among what is allowed.
    pub fn allow_adapter(env: Env, wasm_hash: BytesN<32>, allowed: bool) {
        Self::admin_auth(&env);
        Self::set(&env, Key::Adapter(wasm_hash), &allowed);
    }

    pub fn set_wasms(env: Env, wasms: Wasms) {
        Self::admin_auth(&env);
        env.storage().instance().set(&Key::Wasms, &wasms);
    }

    pub fn set_fee(env: Env, recipient: Address, fee_bps: u32) {
        Self::admin_auth(&env);
        Self::check_fee(&env, fee_bps);
        env.storage()
            .instance()
            .set(&Key::Fee, &(recipient, fee_bps));
    }

    pub fn set_router(env: Env, router: Address) {
        Self::admin_auth(&env);
        env.storage().instance().set(&Key::Router, &router);
    }

    /// Stops routing. Never touches a facility's or vault's own exits.
    pub fn set_paused(env: Env, paused: bool) {
        Self::admin_auth(&env);
        env.storage().instance().set(&Key::Paused, &paused);
    }

    pub fn upgrade(env: Env, wasm_hash: BytesN<32>) {
        Self::admin_auth(&env);
        env.deployer().update_current_contract_wasm(wasm_hash);
    }

    pub fn wasms(env: Env) -> Wasms {
        env.storage().instance().get(&Key::Wasms).unwrap()
    }

    pub fn fee(env: Env) -> (Address, u32) {
        env.storage().instance().get(&Key::Fee).unwrap()
    }

    pub fn router(env: Env) -> Address {
        env.storage().instance().get(&Key::Router).unwrap()
    }

    pub fn active(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&Key::Active)
            .unwrap_or(Vec::new(&env))
    }

    pub fn kind_of(env: Env, contract: Address) -> Option<Kind> {
        env.storage().persistent().get(&Key::Deployed(contract))
    }

    /// Every active facility's quote, net of the protocol fee.
    pub fn quote_all(
        env: Env,
        token_in: Address,
        token_out: Address,
        amount_in: i128,
    ) -> Vec<FacilityQuote> {
        let mut out = Vec::new(&env);
        if Self::paused(&env) {
            return out;
        }
        for facility in Self::active(env.clone()).iter() {
            let q = Self::quote_one(&env, &facility, &token_in, &token_out, amount_in);
            if q > 0 {
                out.push_back(FacilityQuote {
                    facility,
                    amount_out: q,
                });
            }
        }
        out
    }

    /// Standalone entry: `payer` trades with `facility` and signs for it.
    #[allow(clippy::too_many_arguments)]
    pub fn swap(
        env: Env,
        payer: Address,
        recipient: Address,
        token_in: Address,
        token_out: Address,
        amount_in: i128,
        min_out: i128,
        facility: Address,
    ) -> i128 {
        Self::execute(
            &env, &payer, &recipient, &token_in, &token_out, amount_in, min_out, &facility,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn execute(
        env: &Env,
        payer: &Address,
        recipient: &Address,
        token_in: &Address,
        token_out: &Address,
        amount_in: i128,
        min_out: i128,
        facility: &Address,
    ) -> i128 {
        // One signature covers the fee transfer and the facility's pull beneath it.
        payer.require_auth();
        if Self::paused(env) {
            panic_with_error!(env, Error::Paused);
        }
        if Self::active(env.clone()).first_index_of(facility).is_none() {
            panic_with_error!(env, Error::FacilityNotActive);
        }
        let fac = FacilityClient::new(env, facility);
        let base = fac.base();
        let (fee_to, bps) = Self::fee(env.clone());
        let me = env.current_contract_address();
        let out = if token_out == &base {
            // Gross is never below net; the exact net is checked below.
            let gross = fac.trade(payer, &me, token_in, token_out, &amount_in, &min_out);
            let fee = Self::fee_on(env, gross, bps);
            let usd = token::Client::new(env, &base);
            usd.transfer(&me, &fee_to, &fee);
            usd.transfer(&me, recipient, &(gross - fee));
            gross - fee
        } else if token_in == &base {
            let fee = Self::fee_on(env, amount_in, bps);
            token::Client::new(env, &base).transfer(payer, &fee_to, &fee);
            fac.trade(
                payer,
                recipient,
                token_in,
                token_out,
                &(amount_in - fee),
                &min_out,
            )
        } else {
            fac.trade(payer, recipient, token_in, token_out, &amount_in, &min_out)
        };
        if out < min_out {
            panic_with_error!(env, Error::BelowMinOut);
        }
        env.events().publish(
            (symbol_short!("swap"), facility.clone(), payer.clone()),
            (amount_in, out),
        );
        out
    }

    fn quote_one(
        env: &Env,
        facility: &Address,
        token_in: &Address,
        token_out: &Address,
        amount_in: i128,
    ) -> i128 {
        let fac = FacilityClient::new(env, facility);
        let Ok(Ok(base)) = fac.try_base() else {
            return 0;
        };
        let bps = Self::fee(env.clone()).1;
        let q = |amount: i128| match fac.try_quote(token_in, token_out, &amount) {
            Ok(Ok(v)) => v,
            _ => 0,
        };
        if token_out == &base {
            let gross = q(amount_in);
            gross - Self::fee_on(env, gross, bps)
        } else if token_in == &base {
            q(amount_in - Self::fee_on(env, amount_in, bps))
        } else {
            q(amount_in)
        }
    }

    fn deploy(env: &Env, kind: Kind, wasm: BytesN<32>, args: Vec<Val>) -> Address {
        let n: u64 = env.storage().instance().get(&Key::Salt).unwrap_or(0);
        env.storage().instance().set(&Key::Salt, &(n + 1));
        let mut salt = [0u8; 32];
        salt[24..].copy_from_slice(&n.to_be_bytes());
        let addr = env
            .deployer()
            .with_current_contract(BytesN::from_array(env, &salt))
            .deploy_v2(wasm, args);
        Self::set(env, Key::Deployed(addr.clone()), &kind);
        env.events()
            .publish((symbol_short!("created"), addr.clone()), kind);
        addr
    }

    fn expect(env: &Env, contract: &Address, kind: Kind) {
        if Self::kind_of(env.clone(), contract.clone()) != Some(kind) {
            panic_with_error!(env, Error::NotRegistered);
        }
    }

    fn fee_on(env: &Env, amount: i128, bps: u32) -> i128 {
        math::mul_div(env, amount, bps as i128, BPS).unwrap_or(0)
    }

    fn check_fee(env: &Env, bps: u32) {
        if bps > MAX_FEE_BPS {
            panic_with_error!(env, Error::InvalidConfig);
        }
    }

    fn paused(env: &Env) -> bool {
        env.storage().instance().get(&Key::Paused).unwrap_or(false)
    }

    fn admin_auth(env: &Env) {
        Self::admin(env.clone()).require_auth();
    }

    fn set<V: soroban_sdk::IntoVal<Env, Val>>(env: &Env, key: Key, val: &V) {
        env.storage().persistent().set(&key, val);
        env.storage()
            .persistent()
            .extend_ttl(&key, 518_400, 535_680);
    }
}

#[contractimpl]
impl VaultHost for FacilityAggregator {
    fn admin(env: Env) -> Address {
        env.storage().instance().get(&Key::Admin).unwrap()
    }

    fn vault_wasm(env: Env) -> BytesN<32> {
        Self::wasms(env).vault
    }

    /// Only one of our vaults may ask, and only for allowlisted code. The
    /// adapter is owned by that vault and serves no one else.
    fn deploy_adapter(
        env: Env,
        vault: Address,
        wasm_hash: BytesN<32>,
        asset: Address,
        venue: Address,
    ) -> Address {
        vault.require_auth();
        Self::expect(&env, &vault, Kind::Vault);
        let allowed: bool = env
            .storage()
            .persistent()
            .get(&Key::Adapter(wasm_hash.clone()))
            .unwrap_or(false);
        if !allowed {
            panic_with_error!(&env, Error::AdapterNotAllowed);
        }
        let n: u64 = env.storage().instance().get(&Key::Salt).unwrap_or(0);
        env.storage().instance().set(&Key::Salt, &(n + 1));
        let mut salt = [0u8; 32];
        salt[24..].copy_from_slice(&n.to_be_bytes());
        env.deployer()
            .with_current_contract(BytesN::from_array(&env, &salt))
            .deploy_v2(wasm_hash, (vault, asset, venue))
    }
}

#[contractimpl]
impl Aggregator for FacilityAggregator {
    /// The best active facility's net quote. Advisory: `fill` trades with the
    /// facility named in the route, not necessarily this one.
    fn quote(env: Env, token_in: Address, token_out: Address, amount_in: i128) -> i128 {
        let mut best = 0;
        for q in Self::quote_all(env, token_in, token_out, amount_in).iter() {
            best = best.max(q.amount_out);
        }
        best
    }

    /// Router only. `data` is a `FacilityRoute`; the facility pulls the input
    /// from its payer, who signed the router transaction.
    fn fill(
        env: Env,
        recipient: Address,
        token_in: Address,
        token_out: Address,
        amount_in: i128,
        min_amount_out: i128,
        data: Bytes,
    ) -> i128 {
        Self::router(env.clone()).require_auth();
        let route = FacilityRoute::from_xdr(&env, &data)
            .unwrap_or_else(|_| panic_with_error!(&env, Error::InvalidData));
        Self::execute(
            &env,
            &route.payer,
            &recipient,
            &token_in,
            &token_out,
            amount_in,
            min_amount_out,
            &route.facility,
        )
    }
}
