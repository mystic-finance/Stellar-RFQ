#![no_std]
//! The calls a redemption vault (the `caller`) may have its accounts make to
//! an issuer. A rule names a target and function and pins each argument to its
//! template value, except arguments marked free and those marked as the
//! account making the call. Granting is delayed and revoking is not: a rule
//! added or re-enabled counts only after `delay`, during which the guardian or
//! the admin can disable or remove it. The delay is fixed at deployment.

#[cfg(test)]
mod test;

use common::interfaces::CallVerifier as CallVerifierApi;
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, Address,
    Env, IntoVal, Symbol, TryFromVal, Val, Vec,
};

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    InvalidRule = 1,
    UnknownRule = 2,
    NotAuthorized = 3,
}

/// Bit `i` of `free` leaves argument `i` unchecked; bit `i` of `account`
/// requires it to be the account the call runs from. Their templates are ignored.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Rule {
    pub caller: Address,
    pub target: Address,
    pub func: Symbol,
    pub template: Vec<Val>,
    pub free: u32,
    pub account: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Entry {
    pub rule: Rule,
    pub enabled: bool,
    pub valid_after: u64,
}

#[contracttype]
enum Key {
    Admin,
    Guardian,
    Delay,
    Next,
    Entry(u64),
    Ids(Address, Address, Symbol),
}

const MAX_ARGS: u32 = 32;
const THRESHOLD: u32 = 518_400;
const EXTEND: u32 = 535_680;

#[contract]
pub struct CallVerifierContract;

#[contractimpl]
impl CallVerifierContract {
    pub fn __constructor(env: Env, admin: Address, guardian: Address, delay: u64) {
        let s = env.storage().instance();
        s.set(&Key::Admin, &admin);
        s.set(&Key::Guardian, &guardian);
        s.set(&Key::Delay, &delay);
    }

    pub fn delay(env: Env) -> u64 {
        env.storage().instance().get(&Key::Delay).unwrap()
    }

    pub fn entry(env: Env, id: u64) -> Option<Entry> {
        env.storage().persistent().get(&Key::Entry(id))
    }

    pub fn ids(env: Env, caller: Address, target: Address, func: Symbol) -> Vec<u64> {
        env.storage()
            .persistent()
            .get(&Key::Ids(caller, target, func))
            .unwrap_or(Vec::new(&env))
    }

    /// Admin: adds `rule`, live after the delay.
    pub fn add_rule(env: Env, rule: Rule) -> u64 {
        Self::role(&env, Key::Admin).require_auth();
        let mut rule = rule;
        let n = rule.template.len();
        if n > MAX_ARGS || rule.free & rule.account != 0 || (rule.free | rule.account) >> n != 0 {
            panic_with_error!(&env, Error::InvalidRule);
        }
        for i in 0..n {
            if (rule.free | rule.account) & (1 << i) != 0 {
                rule.template.set(i, ().into_val(&env));
            }
        }
        let id: u64 = env.storage().instance().get(&Key::Next).unwrap_or(0);
        env.storage().instance().set(&Key::Next, &(id + 1));
        let k = Key::Ids(rule.caller.clone(), rule.target.clone(), rule.func.clone());
        let mut ids = Self::ids(
            env.clone(),
            rule.caller.clone(),
            rule.target.clone(),
            rule.func.clone(),
        );
        ids.push_back(id);
        Self::put(&env, &k, &ids);
        let entry = Entry {
            rule,
            enabled: true,
            valid_after: env.ledger().timestamp() + Self::delay(env.clone()),
        };
        Self::put(&env, &Key::Entry(id), &entry);
        env.events()
            .publish((symbol_short!("added"), id), entry.valid_after);
        id
    }

    /// Enabling is a grant: the admin only, and the rule waits out the delay
    /// again. Disabling is a revocation: the guardian or the admin, immediate.
    pub fn set_enabled(env: Env, caller: Address, id: u64, enabled: bool) {
        Self::check(&env, &caller, !enabled);
        let mut e = Self::entry_of(&env, id);
        if enabled && !e.enabled {
            e.valid_after = env.ledger().timestamp() + Self::delay(env.clone());
        }
        e.enabled = enabled;
        Self::put(&env, &Key::Entry(id), &e);
        env.events()
            .publish((symbol_short!("enabled"), id), (enabled, e.valid_after));
    }

    /// The guardian or the admin deletes a rule outright.
    pub fn remove(env: Env, caller: Address, id: u64) {
        Self::check(&env, &caller, true);
        let r = Self::entry_of(&env, id).rule;
        let mut ids = Self::ids(
            env.clone(),
            r.caller.clone(),
            r.target.clone(),
            r.func.clone(),
        );
        if let Some(i) = ids.first_index_of(id) {
            ids.remove(i);
        }
        Self::put(&env, &Key::Ids(r.caller, r.target, r.func), &ids);
        env.storage().persistent().remove(&Key::Entry(id));
        env.events().publish((symbol_short!("removed"), id), ());
    }

    fn matches(env: &Env, r: &Rule, account: &Address, args: &Vec<Val>) -> bool {
        if args.len() != r.template.len() {
            return false;
        }
        let mut seen = Vec::<Val>::new(env);
        for (i, v) in args.iter().enumerate() {
            let bit = 1u32 << i;
            if r.account & bit != 0 && Address::try_from_val(env, &v).ok().as_ref() != Some(account)
            {
                return false;
            }
            let ignored = (r.free | r.account) & bit != 0;
            seen.push_back(if ignored { ().into_val(env) } else { v });
        }
        seen == r.template
    }

    fn entry_of(env: &Env, id: u64) -> Entry {
        Self::entry(env.clone(), id).unwrap_or_else(|| panic_with_error!(env, Error::UnknownRule))
    }

    /// `caller` is the admin, or the guardian when `revoking`.
    fn check(env: &Env, caller: &Address, revoking: bool) {
        caller.require_auth();
        let guardian = revoking && caller == &Self::role(env, Key::Guardian);
        if !guardian && caller != &Self::role(env, Key::Admin) {
            panic_with_error!(env, Error::NotAuthorized);
        }
    }

    fn role(env: &Env, k: Key) -> Address {
        env.storage().instance().get(&k).unwrap()
    }

    fn put<V: IntoVal<Env, Val>>(env: &Env, k: &Key, v: &V) {
        env.storage().persistent().set(k, v);
        env.storage().persistent().extend_ttl(k, THRESHOLD, EXTEND);
        env.storage().instance().extend_ttl(THRESHOLD, EXTEND);
    }
}

#[contractimpl]
impl CallVerifierApi for CallVerifierContract {
    fn verify(
        env: Env,
        caller: Address,
        account: Address,
        target: Address,
        func: Symbol,
        args: Vec<Val>,
    ) -> bool {
        let now = env.ledger().timestamp();
        Self::ids(env.clone(), caller, target, func)
            .iter()
            .any(|id| {
                Self::entry(env.clone(), id).is_some_and(|e| {
                    e.enabled
                        && now >= e.valid_after
                        && Self::matches(&env, &e.rule, &account, &args)
                })
            })
    }
}
