use soroban_sdk::{contracttype, Address, BytesN, Env, IntoVal, TryFromVal, Val};

use crate::types::{Config, Listing, OracleCfg, PushedPrice, Reference, Schedule};

const THRESHOLD: u32 = 518_400;
const EXTEND: u32 = 535_680;

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Cfg,
    Ref,
    Paused,
    NextId,
    Keeper(Address),
    Sched(Address),
    Oracle(Address, Address),
    Fallback(Address),
    Filled(BytesN<32>),
    RequestFilled(BytesN<32>),
    SaltCancelled(Address, u64),
    Signer(Address, BytesN<32>),
    Listing(u64),
}

fn get<T: TryFromVal<Env, Val>>(env: &Env, key: &DataKey) -> Option<T> {
    env.storage().persistent().get(key)
}

fn set<T: IntoVal<Env, Val>>(env: &Env, key: DataKey, val: &T) {
    env.storage().persistent().set(&key, val);
    env.storage()
        .persistent()
        .extend_ttl(&key, THRESHOLD, EXTEND);
}

fn iget<T: TryFromVal<Env, Val>>(env: &Env, key: DataKey) -> Option<T> {
    env.storage().instance().get(&key)
}

fn iset<T: IntoVal<Env, Val>>(env: &Env, key: DataKey, val: &T) {
    env.storage().instance().set(&key, val);
}

// --- instance config ---

pub fn has_admin(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Admin)
}

pub fn admin(env: &Env) -> Address {
    iget(env, DataKey::Admin).unwrap()
}

pub fn set_admin(env: &Env, v: &Address) {
    iset(env, DataKey::Admin, v);
}

pub fn config(env: &Env) -> Config {
    iget(env, DataKey::Cfg).unwrap()
}

pub fn set_config(env: &Env, v: &Config) {
    iset(env, DataKey::Cfg, v);
}

pub fn reference(env: &Env) -> Reference {
    iget(env, DataKey::Ref).unwrap()
}

pub fn set_reference(env: &Env, v: &Reference) {
    iset(env, DataKey::Ref, v);
}

pub fn paused(env: &Env) -> bool {
    iget(env, DataKey::Paused).unwrap_or(false)
}

pub fn set_paused(env: &Env, v: bool) {
    iset(env, DataKey::Paused, &v);
}

pub fn next_id(env: &Env) -> u64 {
    let id: u64 = iget(env, DataKey::NextId).unwrap_or(1);
    iset(env, DataKey::NextId, &(id + 1));
    id
}

pub fn extend_instance(env: &Env) {
    env.storage().instance().extend_ttl(THRESHOLD, EXTEND);
}

// --- persistent state ---

pub fn is_keeper(env: &Env, who: &Address) -> bool {
    get(env, &DataKey::Keeper(who.clone())).unwrap_or(false)
}

pub fn set_keeper(env: &Env, who: &Address, v: bool) {
    set(env, DataKey::Keeper(who.clone()), &v);
}

pub fn schedule(env: &Env, asset: &Address) -> Option<Schedule> {
    get(env, &DataKey::Sched(asset.clone()))
}

pub fn set_schedule(env: &Env, asset: &Address, s: &Schedule) {
    set(env, DataKey::Sched(asset.clone()), s);
}

pub fn oracle(env: &Env, base: &Address, quote: &Address) -> Option<OracleCfg> {
    get(env, &DataKey::Oracle(base.clone(), quote.clone()))
}

pub fn set_oracle(env: &Env, base: &Address, quote: &Address, cfg: &Option<OracleCfg>) {
    let key = DataKey::Oracle(base.clone(), quote.clone());
    match cfg {
        Some(c) => set(env, key, c),
        None => env.storage().persistent().remove(&key),
    }
}

pub fn fallback(env: &Env, asset: &Address) -> Option<PushedPrice> {
    get(env, &DataKey::Fallback(asset.clone()))
}

pub fn set_fallback(env: &Env, asset: &Address, p: &PushedPrice) {
    set(env, DataKey::Fallback(asset.clone()), p);
}

pub fn filled(env: &Env, hash: &BytesN<32>) -> i128 {
    get(env, &DataKey::Filled(hash.clone())).unwrap_or(0)
}

pub fn set_filled(env: &Env, hash: &BytesN<32>, amount: i128) {
    set(env, DataKey::Filled(hash.clone()), &amount);
}

pub fn request_filled(env: &Env, hash: &BytesN<32>) -> i128 {
    get(env, &DataKey::RequestFilled(hash.clone())).unwrap_or(0)
}

pub fn set_request_filled(env: &Env, hash: &BytesN<32>, amount: i128) {
    set(env, DataKey::RequestFilled(hash.clone()), &amount);
}

pub fn is_salt_cancelled(env: &Env, signer: &Address, salt: u64) -> bool {
    get(env, &DataKey::SaltCancelled(signer.clone(), salt)).unwrap_or(false)
}

pub fn set_salt_cancelled(env: &Env, signer: &Address, salt: u64) {
    set(env, DataKey::SaltCancelled(signer.clone(), salt), &true);
}

pub fn is_signer(env: &Env, maker: &Address, signer: &BytesN<32>) -> bool {
    get(env, &DataKey::Signer(maker.clone(), signer.clone())).unwrap_or(false)
}

pub fn set_signer(env: &Env, maker: &Address, signer: &BytesN<32>, v: bool) {
    set(env, DataKey::Signer(maker.clone(), signer.clone()), &v);
}

pub fn listing(env: &Env, id: u64) -> Option<Listing> {
    get(env, &DataKey::Listing(id))
}

pub fn set_listing(env: &Env, id: u64, l: &Listing) {
    set(env, DataKey::Listing(id), l);
}
