//! Submit → wait → accept for governed changes, with a guardian who can revoke
//! anything pending. Each contract encodes its changes as one `contracttype`
//! enum and keys them by kind, so it needs three entry points however many
//! parameters it governs. Revoking is immediate; granting waits `delay`.

use soroban_sdk::{
    contracterror, contracttype, panic_with_error, xdr::ToXdr, Address, Env, IntoVal, TryFromVal,
    Val,
};

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum TimelockError {
    NoPending = 110,
    NotReady = 111,
    NotGuardian = 112,
}

#[contracttype]
#[derive(Clone)]
enum TimelockKey {
    Delay,
    Guardian,
    Pending(u32),
}

pub fn init(env: &Env, delay: u64, guardian: &Address) {
    env.storage().instance().set(&TimelockKey::Delay, &delay);
    env.storage()
        .instance()
        .set(&TimelockKey::Guardian, guardian);
}

pub fn delay(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&TimelockKey::Delay)
        .unwrap_or(0)
}

pub fn guardian(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&TimelockKey::Guardian)
        .unwrap()
}

/// The kind for changes to `key`, so changes to different keys queue apart.
/// The high bit is set, leaving small kinds for a contract's fixed slots.
pub fn kind_of<K: IntoVal<Env, Val>>(env: &Env, key: K) -> u32 {
    let h = env.crypto().sha256(&key.to_xdr(env)).to_array();
    u32::from_be_bytes([h[0], h[1], h[2], h[3]]) | 0x8000_0000
}

/// Queues `change` under `kind`, replacing any pending change of that kind.
pub fn submit<T: IntoVal<Env, Val>>(env: &Env, kind: u32, change: &T) -> u64 {
    let valid_at = env.ledger().timestamp() + delay(env);
    let v: Val = change.into_val(env);
    env.storage()
        .persistent()
        .set(&TimelockKey::Pending(kind), &(v, valid_at));
    valid_at
}

/// The change pending under `kind` and when it can be accepted, if any.
pub fn pending<T: TryFromVal<Env, Val>>(env: &Env, kind: u32) -> Option<(T, u64)> {
    let p: Option<(Val, u64)> = env.storage().persistent().get(&TimelockKey::Pending(kind));
    p.and_then(|(v, at)| T::try_from_val(env, &v).ok().map(|c| (c, at)))
}

/// The pending change of `kind`, once its delay has passed. Consumes it.
pub fn take<T: TryFromVal<Env, Val>>(env: &Env, kind: u32) -> T {
    let key = TimelockKey::Pending(kind);
    let (v, valid_at): (Val, u64) = match env.storage().persistent().get(&key) {
        Some(p) => p,
        None => panic_with_error!(env, TimelockError::NoPending),
    };
    if env.ledger().timestamp() < valid_at {
        panic_with_error!(env, TimelockError::NotReady);
    }
    env.storage().persistent().remove(&key);
    T::try_from_val(env, &v).unwrap_or_else(|_| panic_with_error!(env, TimelockError::NoPending))
}

/// The guardian, or `admin` itself, drops a pending change.
pub fn revoke(env: &Env, kind: u32, caller: &Address, admin: &Address) {
    caller.require_auth();
    if caller != admin && caller != &guardian(env) {
        panic_with_error!(env, TimelockError::NotGuardian);
    }
    env.storage()
        .persistent()
        .remove(&TimelockKey::Pending(kind));
}
