#![no_std]
//! Octarine facility: the orchestrator. It holds shares of its vaults (one for
//! the base asset, one per RWA), issues oLP against all of them, and trades
//! RWA ↔ base and RWA ↔ RWA at a rate per day over each RWA's horizon. Tokens
//! never pass through it: the payer pays a vault, a vault pays the recipient.
//! See docs/FACILITY_V2_STELLAR.md.

mod errors;
mod types;

#[cfg(test)]
mod test;

use common::interfaces::{OracleClient, VaultHostClient};
use common::math::{BPS, DENOM, ONE};
use common::timelock;
use shares::{md, Snapshot};
use soroban_sdk::token::TokenInterface;
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{
    contract, contractclient, contractimpl, contracttype, panic_with_error, symbol_short, token,
    Address, Env, Vec,
};

pub use errors::Error;
pub use types::{Change, FacilityConfig, Redemption, Rwa, Terms};

/// The vault functions the facility uses.
#[contractclient(name = "VaultClient")]
pub trait VaultFns {
    fn query_asset(env: Env) -> Address;
    fn deposit(env: Env, assets: i128, receiver: Address, from: Address, operator: Address)
        -> i128;
    fn withdraw(
        env: Env,
        assets: i128,
        receiver: Address,
        owner: Address,
        operator: Address,
    ) -> i128;
    fn convert_to_assets(env: Env, shares: i128) -> i128;
    fn max_withdraw(env: Env, owner: Address) -> i128;
    fn balance(env: Env, id: Address) -> i128;
}

const MAX_RWAS: u32 = 8;
const MAX_FEE_BPS: u32 = 5_000;

#[contracttype]
enum Key {
    Host,
    Base,
    Config,
    Queue,
    Rwas,
    Locked,
    Paused,
    Rwa(Address),
    Redemption(Address),
}

/// A gain or loss against the oracle mark, in base units.
struct Trade {
    out: i128,
    gain: i128,
}

#[contract]
pub struct Facility;

shares::share_token!(Facility);

#[contractimpl]
impl Facility {
    pub fn __constructor(env: Env, host: Address, config: FacilityConfig) {
        if config.curator_fee_bps > MAX_FEE_BPS {
            panic_with_error!(&env, Error::InvalidConfig);
        }
        let base = VaultClient::new(&env, &config.base_vault).query_asset();
        let decimals = token::Client::new(&env, &base).decimals();
        shares::init(&env, decimals, config.name.clone(), config.symbol.clone());
        timelock::init(&env, config.timelock, &config.guardian);
        env.storage().instance().set(&Key::Host, &host);
        env.storage().instance().set(&Key::Base, &base);
        env.storage().instance().set(&Key::Config, &config);
    }

    pub fn config(env: Env) -> FacilityConfig {
        env.storage().instance().get(&Key::Config).unwrap()
    }

    pub fn base(env: Env) -> Address {
        env.storage().instance().get(&Key::Base).unwrap()
    }

    pub fn base_vault(env: Env) -> Address {
        Self::config(env).base_vault
    }

    pub fn rwas(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&Key::Rwas)
            .unwrap_or(Vec::new(&env))
    }

    pub fn rwa(env: Env, rwa: Address) -> Option<Rwa> {
        env.storage().persistent().get(&Key::Rwa(rwa))
    }

    pub fn redemption(env: Env, rwa: Address) -> Option<Redemption> {
        env.storage().persistent().get(&Key::Redemption(rwa))
    }

    pub fn queue(env: Env) -> Option<Address> {
        env.storage().instance().get(&Key::Queue)
    }

    pub fn total_assets(env: Env) -> i128 {
        Self::snapshot(&env).nav
    }

    pub fn total_supply(env: Env) -> i128 {
        Self::snapshot(&env).supply
    }

    pub fn convert_to_assets(env: Env, shares: i128) -> i128 {
        shares::to_assets(&env, &Self::snapshot(&env), shares, false)
    }

    pub fn convert_to_shares(env: Env, assets: i128) -> i128 {
        shares::to_shares(&env, &Self::snapshot(&env), assets, false)
    }

    /// Gain against the oracle mark not yet released to oLP holders.
    pub fn locked(env: Env) -> i128 {
        Self::locked_now(&env)
    }

    pub fn max_withdraw(env: Env, owner: Address) -> i128 {
        let s = Self::snapshot(&env);
        shares::to_assets(&env, &s, shares::balance(&env, &owner), false)
            .min(Self::free(&env, &s, &owner))
    }

    /// What `trade` would pay for `amount_in`, or 0 if it would not fill.
    pub fn quote(env: Env, token_in: Address, token_out: Address, amount_in: i128) -> i128 {
        Self::price_trade(&env, &token_in, &token_out, amount_in).map_or(0, |t| t.out)
    }

    pub fn deposit(
        env: Env,
        assets: i128,
        receiver: Address,
        from: Address,
        operator: Address,
    ) -> i128 {
        operator.require_auth();
        if operator != from {
            from.require_auth();
        }
        if !shares::admitted(&env, &receiver) {
            panic_with_error!(&env, Error::NotAllowlisted);
        }
        let s = Self::accrue_now(&env);
        let got = Self::take(&env, &Self::config(env.clone()).base_vault, &from, assets);
        let minted = shares::to_shares(&env, &s, got, false);
        if minted <= 0 {
            panic_with_error!(&env, Error::InvalidAmount);
        }
        shares::mint(&env, &receiver, minted);
        minted
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
        Self::exit(&env, &s, &owner, &operator, &receiver, burned, assets);
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
        Self::exit(&env, &s, &owner, &operator, &receiver, shares, assets);
        assets
    }

    pub fn accrue(env: Env) {
        Self::accrue_now(&env);
    }

    /// `payer` pays `amount_in` of `token_in` straight into its vault, and the
    /// output vault pays `recipient`. Any caller may route it; the payer signs.
    pub fn trade(
        env: Env,
        payer: Address,
        recipient: Address,
        token_in: Address,
        token_out: Address,
        amount_in: i128,
        min_out: i128,
    ) -> i128 {
        payer.require_auth();
        let s = Self::accrue_now(&env);
        let t = Self::price_trade(&env, &token_in, &token_out, amount_in)
            .unwrap_or_else(|e| panic_with_error!(&env, e));
        if t.out < min_out {
            panic_with_error!(&env, Error::BelowMinOut);
        }
        let (vin, vout) = (
            Self::vault_of(&env, &token_in),
            Self::vault_of(&env, &token_out),
        );
        if Self::take(&env, &vin, &payer, amount_in) + 1 < amount_in {
            panic_with_error!(&env, Error::InvalidAmount);
        }
        if vout == Self::config(env.clone()).base_vault && t.out > Self::free(&env, &s, &recipient)
        {
            panic_with_error!(&env, Error::InsufficientLiquidity);
        }
        VaultClient::new(&env, &vout).withdraw(
            &t.out,
            &recipient,
            &env.current_contract_address(),
            &env.current_contract_address(),
        );
        Self::book_gain(&env, t.gain);
        Self::check_cap(&env, &token_in, s.nav);
        env.events().publish(
            (symbol_short!("trade"), token_in, token_out),
            (amount_in, t.out, payer),
        );
        t.out
    }

    /// Sends facility-owned RWA to its redemption address; NAV carries it as a
    /// receivable at its oracle value, so the share price does not move.
    pub fn book(env: Env, rwa: Address, amount: i128) {
        Self::manager_auth(&env);
        Self::accrue_now(&env);
        if Self::redemption(env.clone(), rwa.clone()).is_some() {
            panic_with_error!(&env, Error::RedemptionOpen);
        }
        let r = Self::listed(&env, &rwa);
        let carried = md(
            &env,
            amount,
            Self::price(&env, &rwa, &r.terms).unwrap_or_else(|e| panic_with_error!(&env, e)),
            ONE,
        );
        let me = env.current_contract_address();
        VaultClient::new(&env, &r.vault).withdraw(&amount, &r.terms.redemption, &me, &me);
        Self::set(
            &env,
            Key::Redemption(rwa.clone()),
            &Redemption { amount, carried },
        );
        env.events()
            .publish((symbol_short!("booked"), rwa), (amount, carried));
    }

    /// All-or-nothing: less than the carried value reverts. A surplus vests.
    pub fn settle(env: Env, rwa: Address, proceeds: i128) {
        let c = Self::manager_auth(&env);
        Self::accrue_now(&env);
        let rec = Self::open(&env, &rwa);
        if proceeds < rec.carried {
            panic_with_error!(&env, Error::NotFullyRepaid);
        }
        Self::take(&env, &c.base_vault, &c.manager, proceeds);
        env.storage()
            .persistent()
            .remove(&Key::Redemption(rwa.clone()));
        Self::book_gain(&env, proceeds - rec.carried);
        env.events()
            .publish((symbol_short!("settled"), rwa), (proceeds, rec.carried));
    }

    /// Protocol admin only: books a redemption the issuer will not honour in
    /// full, recovering `recovered` (possibly 0).
    pub fn write_down(env: Env, rwa: Address, recovered: i128) {
        let admin = VaultHostClient::new(&env, &Self::host(&env)).admin();
        admin.require_auth();
        Self::accrue_now(&env);
        let rec = Self::open(&env, &rwa);
        if recovered < 0 || recovered >= rec.carried {
            panic_with_error!(&env, Error::InvalidAmount);
        }
        if recovered > 0 {
            Self::take(
                &env,
                &Self::config(env.clone()).base_vault,
                &admin,
                recovered,
            );
        }
        env.storage()
            .persistent()
            .remove(&Key::Redemption(rwa.clone()));
        Self::book_gain(&env, recovered - rec.carried);
        env.events()
            .publish((symbol_short!("writedn"), rwa), (recovered, rec.carried));
    }

    /// Returns the change's kind (to accept or revoke it) and when it unlocks.
    pub fn submit(env: Env, change: Change) -> (u32, u64) {
        let c = Self::curator_auth(&env);
        match &change {
            Change::CuratorFee(bps) if *bps > MAX_FEE_BPS => {
                panic_with_error!(&env, Error::InvalidConfig)
            }
            Change::Rwa(rwa, vault, t) => Self::validate(&env, &c, rwa, vault, t),
            _ => {}
        }
        let kind = Self::kind(&env, &change);
        (kind, timelock::submit(&env, kind, &change))
    }

    pub fn accept(env: Env, kind: u32) {
        let mut c = Self::config(env.clone());
        match timelock::take::<Change>(&env, kind) {
            Change::CuratorFee(bps) => {
                Self::accrue_now(&env);
                c.curator_fee_bps = bps;
            }
            Change::Vest(secs) => c.vest_seconds = secs,
            Change::Queue(q) => env.storage().instance().set(&Key::Queue, &q),
            Change::Rwa(rwa, vault, terms) => Self::list(&env, rwa, vault, terms),
        }
        env.storage().instance().set(&Key::Config, &c);
    }

    pub fn pending(env: Env, kind: u32) -> Option<(Change, u64)> {
        timelock::pending(&env, kind)
    }

    pub fn revoke(env: Env, kind: u32, caller: Address) {
        timelock::revoke(&env, kind, &caller, &Self::config(env.clone()).curator);
    }

    /// Drops an RWA the facility no longer holds or is redeeming.
    pub fn remove_rwa(env: Env, rwa: Address) {
        Self::curator_auth(&env);
        let r = Self::listed(&env, &rwa);
        if VaultClient::new(&env, &r.vault).balance(&env.current_contract_address()) > 0
            || Self::redemption(env.clone(), rwa.clone()).is_some()
        {
            panic_with_error!(&env, Error::StillHeld);
        }
        let mut rwas = Self::rwas(env.clone());
        rwas.remove(rwas.first_index_of(&rwa).unwrap());
        env.storage().instance().set(&Key::Rwas, &rwas);
        env.storage().persistent().remove(&Key::Rwa(rwa));
    }

    pub fn paused(env: Env) -> bool {
        env.storage().instance().get(&Key::Paused).unwrap_or(false)
    }

    /// Curator or guardian, no timelock: stops trading, never exits.
    pub fn set_paused(env: Env, caller: Address, paused: bool) {
        let c = Self::config(env.clone());
        if caller != c.curator && caller != c.guardian {
            panic_with_error!(&env, Error::InvalidConfig);
        }
        caller.require_auth();
        env.storage().instance().set(&Key::Paused, &paused);
        env.events().publish((symbol_short!("paused"),), paused);
    }

    pub fn set_allowlist(env: Env, enabled: bool) {
        Self::curator_auth(&env);
        shares::set_allowlist(&env, enabled);
    }

    pub fn set_member(env: Env, who: Address, allowed: bool) {
        Self::curator_auth(&env);
        shares::set_member(&env, &who, allowed);
    }

    fn price_trade(
        env: &Env,
        token_in: &Address,
        token_out: &Address,
        q: i128,
    ) -> Result<Trade, Error> {
        if Self::paused(env.clone()) {
            return Err(Error::Paused);
        }
        if q <= 0 || token_in == token_out {
            return Err(Error::InvalidAmount);
        }
        let base = Self::base(env.clone());
        // The base value the input fetches, and its oracle mark.
        let (value, mark_in) = if token_in == &base {
            (q, q)
        } else {
            Self::bid(env, token_in, q)?
        };
        let t = if token_out == &base {
            Trade {
                out: value,
                gain: mark_in - value,
            }
        } else {
            let (units, mark_out) = Self::ask(env, token_out, value)?;
            Trade {
                out: units,
                gain: mark_in - mark_out,
            }
        };
        if t.out <= 0 {
            return Err(Error::InvalidAmount);
        }
        Ok(t)
    }

    /// Selling `q` of `rwa` to the facility: base paid, and its oracle value.
    fn bid(env: &Env, rwa: &Address, q: i128) -> Result<(i128, i128), Error> {
        let t = Self::accepted(env, rwa)?;
        let mark = md(env, q, Self::price(env, rwa, &t)?, ONE);
        let discount = t.bid_rate as i128 * Self::horizon(env, &t)?;
        Ok((md(env, mark, (DENOM - discount).max(0), DENOM), mark))
    }

    /// Buying `rwa` from the facility with `value` of base: units, and their
    /// oracle value. Bounded by what the facility holds.
    fn ask(env: &Env, rwa: &Address, value: i128) -> Result<(i128, i128), Error> {
        let t = Self::accepted(env, rwa)?;
        let p = Self::price(env, rwa, &t)?;
        let factor = DENOM - t.ask_rate as i128 * Self::horizon(env, &t)?;
        if factor <= 0 {
            return Err(Error::InvalidConfig);
        }
        let units = md(env, md(env, value, ONE, p), DENOM, factor);
        let held = VaultClient::new(env, &Self::listed(env, rwa).vault)
            .max_withdraw(&env.current_contract_address());
        if units > held {
            return Err(Error::InsufficientLiquidity);
        }
        Ok((units, md(env, units, p, ONE)))
    }

    fn accepted(env: &Env, rwa: &Address) -> Result<Terms, Error> {
        match Self::rwa(env.clone(), rwa.clone()) {
            Some(r) if r.terms.accepted => Ok(r.terms),
            Some(_) => Err(Error::NotAccepted),
            None => Err(Error::UnsupportedPair),
        }
    }

    fn price(env: &Env, rwa: &Address, t: &Terms) -> Result<i128, Error> {
        match OracleClient::new(env, &t.oracle).try_get_price(rwa, &Self::base(env.clone())) {
            Ok(Ok(p))
                if p.price > 0 && env.ledger().timestamp() <= p.timestamp + t.max_price_age =>
            {
                Ok(p.price)
            }
            _ => Err(Error::NoPrice),
        }
    }

    fn horizon(env: &Env, t: &Terms) -> Result<i128, Error> {
        common::schedule::seconds_for(env.ledger().timestamp(), &t.schedule)
            .map(|h| (h as i128).max(1))
            .ok_or(Error::InvalidConfig)
    }

    /// Base vault position + RWA positions at the oracle + receivables, less
    /// gain still vesting. Panics without a price for an RWA the facility holds.
    fn gross(env: &Env) -> i128 {
        let me = env.current_contract_address();
        let mut nav = Self::position(env, &Self::config(env.clone()).base_vault, &me);
        for rwa in Self::rwas(env.clone()).iter() {
            let r = Self::listed(env, &rwa);
            let units = Self::position(env, &r.vault, &me);
            if units > 0 {
                nav += md(
                    env,
                    units,
                    Self::price(env, &rwa, &r.terms).unwrap_or_else(|e| panic_with_error!(env, e)),
                    ONE,
                );
            }
            nav += Self::redemption(env.clone(), rwa).map_or(0, |r| r.carried);
        }
        nav - Self::locked_now(env)
    }

    fn position(env: &Env, vault: &Address, who: &Address) -> i128 {
        let v = VaultClient::new(env, vault);
        match v.balance(who) {
            0 => 0,
            s => v.convert_to_assets(&s),
        }
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

    /// Base payable now, less what the queue is owed. The queue itself may take it all.
    fn free(env: &Env, s: &Snapshot, who: &Address) -> i128 {
        let me = env.current_contract_address();
        let available =
            VaultClient::new(env, &Self::config(env.clone()).base_vault).max_withdraw(&me);
        match Self::queue(env.clone()) {
            Some(q) if &q != who => {
                (available - shares::to_assets(env, s, shares::balance(env, &q), true)).max(0)
            }
            _ => available,
        }
    }

    fn check_cap(env: &Env, rwa: &Address, nav: i128) {
        if let Some(r) = Self::rwa(env.clone(), rwa.clone()) {
            let held = Self::position(env, &r.vault, &env.current_contract_address());
            let value = md(env, held, Self::price(env, rwa, &r.terms).unwrap_or(0), ONE);
            if value > md(env, nav.max(0), r.terms.max_bps as i128, BPS) {
                panic_with_error!(env, Error::CapExceeded);
            }
        }
    }

    fn locked_state(env: &Env) -> (i128, u64, u64) {
        env.storage()
            .instance()
            .get(&Key::Locked)
            .unwrap_or((0, 0, 0))
    }

    fn locked_now(env: &Env) -> i128 {
        let (locked, last, end) = Self::locked_state(env);
        let now = env.ledger().timestamp();
        if now >= end || locked == 0 {
            return 0;
        }
        md(env, locked, (end - now) as i128, (end - last) as i128)
    }

    /// A gain locks and unlocks linearly; the new end is weighted by size. A
    /// loss is absorbed by locked gain first.
    fn book_gain(env: &Env, gain: i128) {
        let now = env.ledger().timestamp();
        let (_, _, end) = Self::locked_state(env);
        let current = Self::locked_now(env);
        let vest = Self::config(env.clone()).vest_seconds as i128;
        let (locked, end) = if gain >= 0 {
            let total = current + gain;
            let left = end.saturating_sub(now) as i128;
            let span = if total > 0 {
                md(env, current * left + gain * vest, 1, total)
            } else {
                0
            };
            (total, now + span as u64)
        } else {
            ((current + gain).max(0), end)
        };
        env.storage()
            .instance()
            .set(&Key::Locked, &(locked, now, end));
    }

    /// Pays `assets` from `from` into `vault` for the facility; returns the
    /// base-equivalent value it now holds there in addition.
    fn take(env: &Env, vault: &Address, from: &Address, assets: i128) -> i128 {
        let me = env.current_contract_address();
        let before = Self::position(env, vault, &me);
        VaultClient::new(env, vault).deposit(&assets, &me, from, &me);
        Self::position(env, vault, &me) - before
    }

    fn exit(
        env: &Env,
        s: &Snapshot,
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
        if assets > Self::free(env, s, owner) {
            panic_with_error!(env, Error::InsufficientLiquidity);
        }
        shares::burn(env, owner, burned);
        let me = env.current_contract_address();
        VaultClient::new(env, &Self::config(env.clone()).base_vault)
            .withdraw(&assets, receiver, &me, &me);
    }

    fn vault_of(env: &Env, token: &Address) -> Address {
        let c = Self::config(env.clone());
        if token == &Self::base(env.clone()) {
            return c.base_vault;
        }
        Self::listed(env, token).vault
    }

    fn listed(env: &Env, rwa: &Address) -> Rwa {
        Self::rwa(env.clone(), rwa.clone())
            .unwrap_or_else(|| panic_with_error!(env, Error::UnsupportedPair))
    }

    fn open(env: &Env, rwa: &Address) -> Redemption {
        Self::redemption(env.clone(), rwa.clone())
            .unwrap_or_else(|| panic_with_error!(env, Error::NoRedemption))
    }

    fn validate(env: &Env, c: &FacilityConfig, rwa: &Address, vault: &Address, t: &Terms) {
        let ok = VaultClient::new(env, vault).query_asset() == *rwa
            && *vault != c.base_vault
            && t.ask_rate <= t.bid_rate
            && t.bid_rate <= t.schedule.max_bps_per_day
            && t.max_bps as i128 <= BPS
            && common::schedule::is_valid(env.ledger().timestamp(), &t.schedule);
        if !ok {
            panic_with_error!(env, Error::InvalidConfig);
        }
    }

    fn list(env: &Env, rwa: Address, vault: Address, terms: Terms) {
        let mut rwas = Self::rwas(env.clone());
        match Self::rwa(env.clone(), rwa.clone()) {
            Some(old)
                if old.vault != vault
                    && VaultClient::new(env, &old.vault)
                        .balance(&env.current_contract_address())
                        > 0 =>
            {
                panic_with_error!(env, Error::StillHeld)
            }
            Some(_) => {}
            None if rwas.len() >= MAX_RWAS => panic_with_error!(env, Error::TooManyRwas),
            None => rwas.push_back(rwa.clone()),
        }
        env.storage().instance().set(&Key::Rwas, &rwas);
        Self::set(env, Key::Rwa(rwa.clone()), &Rwa { vault, terms });
        env.events().publish((symbol_short!("rwa"), rwa), ());
    }

    /// RWA changes are keyed by the RWA, so each can be pending independently.
    fn kind(env: &Env, c: &Change) -> u32 {
        match c {
            Change::CuratorFee(_) => 0,
            Change::Vest(_) => 1,
            Change::Queue(_) => 2,
            Change::Rwa(rwa, ..) => {
                let h = env.crypto().sha256(&rwa.clone().to_xdr(env)).to_array();
                0x8000_0000 | u32::from_be_bytes([h[0], h[1], h[2], h[3]])
            }
        }
    }

    fn set<T: soroban_sdk::IntoVal<Env, soroban_sdk::Val>>(env: &Env, k: Key, v: &T) {
        env.storage().persistent().set(&k, v);
        env.storage().persistent().extend_ttl(&k, 518_400, 535_680);
    }

    fn host(env: &Env) -> Address {
        env.storage().instance().get(&Key::Host).unwrap()
    }

    fn curator_auth(env: &Env) -> FacilityConfig {
        let c = Self::config(env.clone());
        c.curator.require_auth();
        c
    }

    fn manager_auth(env: &Env) -> FacilityConfig {
        let c = Self::config(env.clone());
        c.manager.require_auth();
        c
    }
}
