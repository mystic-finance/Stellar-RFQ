#![no_std]
//! Batched issuer redemption for one RWA, paid in one stable. A request joins
//! the open batch and mints that batch's claim token (CT) one for one; the CT
//! trades freely until the batch settles, then any holder burns it for a share
//! of the proceeds. A batch reaches the issuer one of two ways, by `mode`:
//! AUTO, where the settler sends it from a redemption account through verified
//! calls and the proceeds are what actually arrive; or MANUAL, where the
//! operator takes it to an allowed destination, redeems it off-chain and pays
//! the proceeds in, no less than the NAV floor. Governance is timelocked. See
//! docs/REDEMPTION_VAULT.md.

#[cfg(test)]
mod test;

use common::interfaces::{AccountClient, CallVerifierClient, ClaimTokenClient, OracleClient};
use common::math::{mul_div, mul_div_ceil, BPS, ONE};
use common::timelock;
use common::types::PriceData;
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, token,
    Address, BytesN, Env, IntoVal, String, Symbol, Val, Vec,
};

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    InvalidAmount = 1,
    WrongStatus = 2,
    WrongMode = 3,
    NoPrice = 4,
    NotAllowed = 5,
    IssuerTookWrongAmount = 6,
    RwaMoved = 7,
    ProceedsTooLow = 8,
    CallNotVerified = 9,
    NoFreeAccount = 10,
    BatchesInFlight = 11,
    InvalidSettings = 12,
    Paused = 13,
    NotAuthorized = 14,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Auto,
    Manual,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    Open,
    Sent,
    Settled,
}

/// Fixed at deployment.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Init {
    pub rwa: Address,
    pub stable: Address,
    pub ct_wasm: BytesN<32>,
    pub account_wasm: BytesN<32>,
}

/// `settler` runs AUTO batches, `operator` MANUAL ones; `guardian` can revoke
/// any pending change and pause requests.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Roles {
    pub admin: Address,
    pub guardian: Address,
    pub settler: Address,
    pub operator: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Settings {
    /// Changing it needs no batch with the issuer.
    pub mode: Mode,
    /// The issuer fixes the cash at request, so a batch is struck at the NAV it
    /// leaves at; otherwise it is worth the NAV at settlement. Changing it needs
    /// no batch with the issuer.
    pub strike_at_send: bool,
    pub verifier: Option<Address>,
    /// Who may hold the open batch's CT and every later one; `None` is anyone.
    pub gate: Option<Address>,
    /// The RWA's NAV in the stable, and the oldest reading accepted.
    pub oracle: Option<Address>,
    pub max_age: u64,
    pub fee_bps: u32,
    pub fee_recipient: Option<Address>,
    /// How far below the NAV floor a MANUAL settlement may land.
    pub floor_bps: u32,
    /// Where the operator may take a MANUAL batch.
    pub destinations: Vec<Address>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Change {
    Settings(Settings),
    Roles(Roles),
    /// Sends `token` the vault holds beyond what it owes to `to`.
    Skim(Address, Address),
    Delay(u64),
}

/// `account` holds an AUTO batch at the issuer. `unsent` is RWA the issuer
/// handed back: waiting to be sent again or, once settled, owed to holders in
/// kind. `remaining` is the proceeds left for holders still to claim.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Batch {
    pub status: Status,
    pub ct: Address,
    pub account: Option<Address>,
    pub units: i128,
    pub unsent: i128,
    pub proceeds: i128,
    pub fee: i128,
    pub remaining: i128,
    pub strike: i128,
    pub nav_start: i128,
    pub nav_end: i128,
    pub sent_at: u64,
    pub settled_at: u64,
}

/// RWA in open batches; proceeds and RWA owed to holders of settled ones.
#[contracttype]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Totals {
    pub pooled: i128,
    pub proceeds: i128,
    pub rwa: i128,
    pub sent: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Call {
    pub target: Address,
    pub func: Symbol,
    pub args: Vec<Val>,
}

#[contracttype]
enum Key {
    Init,
    Roles,
    Settings,
    Totals,
    Paused,
    Open,
    Next,
    Accounts,
    Batch(u64),
    Busy(Address),
    Ct(Address),
}

const MAX_FEE_BPS: u32 = 2_000;
const CT_SALT: u8 = 1;
const ACCOUNT_SALT: u8 = 2;
const THRESHOLD: u32 = 518_400;
const EXTEND: u32 = 535_680;

#[contract]
pub struct RedemptionVault;

#[contractimpl]
impl RedemptionVault {
    pub fn __constructor(env: Env, init: Init, roles: Roles, settings: Settings, delay: u64) {
        Self::check_settings(&env, &settings);
        timelock::init(&env, delay, &roles.guardian);
        let s = env.storage().instance();
        s.set(&Key::Init, &init);
        s.set(&Key::Roles, &roles);
        s.set(&Key::Settings, &settings);
        s.set(&Key::Next, &1u64);
        Self::add_account(&env);
    }

    // ---- holders

    /// Pools `units` of `owner`'s RWA into the open batch and mints its CT to `controller`.
    pub fn request_redeem(env: Env, owner: Address, units: i128, controller: Address) -> u64 {
        owner.require_auth();
        if Self::paused(env.clone()) {
            panic_with_error!(&env, Error::Paused);
        }
        let me = env.current_contract_address();
        if units <= 0 || controller == me {
            panic_with_error!(&env, Error::InvalidAmount);
        }
        token::Client::new(&env, &Self::init(env.clone()).rwa).transfer(&owner, &me, &units);
        let (id, mut b) = Self::open(&env);
        b.units += units;
        Self::put(&env, &Key::Batch(id), &b);
        Self::tally(&env, |t| t.pooled += units);
        ClaimTokenClient::new(&env, &b.ct).mint(&controller, &units);
        env.events()
            .publish((symbol_short!("request"), id, controller), units);
        id
    }

    /// Burns `amount` of `owner`'s CT while the batch is open and returns the RWA to `receiver`.
    pub fn cancel(env: Env, owner: Address, id: u64, amount: i128, receiver: Address) {
        owner.require_auth();
        let mut b = Self::with_status(&env, id, Status::Open);
        if amount <= 0 || amount > b.units {
            panic_with_error!(&env, Error::InvalidAmount);
        }
        ClaimTokenClient::new(&env, &b.ct).burn_claim(&owner, &amount);
        b.units -= amount;
        Self::put(&env, &Key::Batch(id), &b);
        Self::tally(&env, |t| t.pooled -= amount);
        Self::send(&env, &Self::init(env.clone()).rwa, &receiver, amount);
        env.events()
            .publish((symbol_short!("cancel"), id, owner), amount);
    }

    /// Burns `shares` of `owner`'s CT of a settled batch for its share of what
    /// the batch has left, so the last holder takes the rounding dust.
    pub fn claim(env: Env, owner: Address, id: u64, shares: i128, receiver: Address) -> i128 {
        owner.require_auth();
        let mut b = Self::with_status(&env, id, Status::Settled);
        if shares <= 0 {
            panic_with_error!(&env, Error::InvalidAmount);
        }
        let ct = ClaimTokenClient::new(&env, &b.ct);
        let supply = ct.total_supply();
        let assets = Self::md(&env, shares, b.remaining, supply);
        let units = Self::md(&env, shares, b.unsent, supply);
        ct.burn_claim(&owner, &shares);
        b.remaining -= assets;
        b.unsent -= units;
        Self::put(&env, &Key::Batch(id), &b);
        Self::tally(&env, |t| {
            t.proceeds -= assets;
            t.rwa -= units;
        });
        let init = Self::init(env.clone());
        Self::send(&env, &init.stable, &receiver, assets);
        Self::send(&env, &init.rwa, &receiver, units);
        env.events()
            .publish((symbol_short!("claim"), id, owner), (assets, units));
        assets
    }

    // ---- AUTO: the settler, through verified calls

    /// Sends an open batch to the issuer from a free account, or resends what
    /// the issuer handed back. The calls must take exactly the batch's RWA;
    /// `spender`, if any, is allowed to pull it for their duration.
    pub fn redeem_batch(env: Env, id: u64, spender: Option<Address>, calls: Vec<Call>) {
        Self::roles(env.clone()).settler.require_auth();
        let mut b = Self::batch_of(&env, id);
        if b.status == Status::Sent {
            return Self::resend(&env, id, b, spender, calls);
        }
        if b.status != Status::Open {
            panic_with_error!(&env, Error::WrongStatus);
        }
        let s = Self::mode(&env, Mode::Auto);
        let account = Self::free_account(&env);
        let rwa = token::Client::new(&env, &Self::init(env.clone()).rwa);
        let me = env.current_contract_address();
        rwa.transfer(&me, &account, &b.units);
        Self::send_from(&env, &account, b.units, spender, &calls);
        Self::put(&env, &Key::Busy(account.clone()), &id);
        Self::tally(&env, |t| t.pooled -= b.units);
        let t = Self::totals(env.clone());
        if rwa.balance(&me) < t.pooled + t.rwa {
            panic_with_error!(&env, Error::RwaMoved);
        }
        b.account = Some(account);
        Self::mark_sent(&env, id, b, &s);
    }

    /// Runs the calls that collect the proceeds, then books whatever stable
    /// the account received, at least `min_proceeds`.
    pub fn settle(env: Env, id: u64, calls: Vec<Call>, min_proceeds: i128) {
        Self::roles(env.clone()).settler.require_auth();
        let b = Self::with_status(&env, id, Status::Sent);
        let account = b
            .account
            .clone()
            .unwrap_or_else(|| panic_with_error!(&env, Error::WrongMode));
        let init = Self::init(env.clone());
        let me = env.current_contract_address();
        let (rwa, stable) = (
            token::Client::new(&env, &init.rwa),
            token::Client::new(&env, &init.stable),
        );
        let rwa_before = rwa.balance(&me) + b.unsent;
        let stable_before = stable.balance(&me);
        Self::run_calls(&env, &account, &calls);
        let acct = AccountClient::new(&env, &account);
        acct.sweep(&init.stable, &me);
        if b.unsent != 0 {
            acct.execute(
                &init.rwa,
                &symbol_short!("transfer"),
                &(account.clone(), me.clone(), b.unsent).into_val(&env),
            );
        }
        env.storage().persistent().remove(&Key::Busy(account));
        if rwa.balance(&me) != rwa_before {
            panic_with_error!(&env, Error::RwaMoved);
        }
        let proceeds = stable.balance(&me) - stable_before;
        if proceeds < min_proceeds {
            panic_with_error!(&env, Error::ProceedsTooLow);
        }
        Self::finish(&env, id, b, proceeds);
    }

    /// The issuer handed back `returned` RWA: held for a resend or, once all of
    /// the batch is back, the batch is open again.
    pub fn reopen(env: Env, id: u64, calls: Vec<Call>, returned: i128) {
        Self::roles(env.clone()).settler.require_auth();
        let mut b = Self::with_status(&env, id, Status::Sent);
        let account = b
            .account
            .clone()
            .unwrap_or_else(|| panic_with_error!(&env, Error::WrongMode));
        let unsent = b.unsent + returned;
        if returned <= 0 || unsent > b.units {
            panic_with_error!(&env, Error::InvalidAmount);
        }
        Self::run_calls(&env, &account, &calls);
        let rwa = Self::init(env.clone()).rwa;
        if token::Client::new(&env, &rwa).balance(&account) < unsent {
            panic_with_error!(&env, Error::InvalidAmount);
        }
        b.unsent = unsent;
        if unsent == b.units {
            AccountClient::new(&env, &account).sweep(&rwa, &env.current_contract_address());
            env.storage().persistent().remove(&Key::Busy(account));
            Self::tally(&env, |t| {
                t.pooled += b.units;
                t.sent -= 1;
            });
            b.status = Status::Open;
            b.account = None;
            b.unsent = 0;
            b.strike = 0;
        }
        Self::put(&env, &Key::Batch(id), &b);
        env.events()
            .publish((symbol_short!("reopened"), id), returned);
    }

    // ---- MANUAL: the operator

    /// Takes an open batch's RWA to `to`, an allowed destination, to redeem off-chain.
    pub fn withdraw_batch(env: Env, id: u64, to: Address) {
        Self::roles(env.clone()).operator.require_auth();
        let s = Self::mode(&env, Mode::Manual);
        if !s.destinations.contains(&to) {
            panic_with_error!(&env, Error::NotAllowed);
        }
        let b = Self::with_status(&env, id, Status::Open);
        Self::send(&env, &Self::init(env.clone()).rwa, &to, b.units);
        Self::tally(&env, |t| t.pooled -= b.units);
        Self::mark_sent(&env, id, b, &s);
    }

    /// Pays in a MANUAL batch's `proceeds` and any RWA the issuer `returned`,
    /// from the operator. The proceeds must reach the NAV floor: the strike, or
    /// the NAV now, on the units redeemed, less `floor_bps`, rounded up.
    pub fn settle_manual(env: Env, id: u64, proceeds: i128, returned: i128) {
        let operator = Self::roles(env.clone()).operator;
        operator.require_auth();
        let mut b = Self::with_status(&env, id, Status::Sent);
        if b.account.is_some() {
            panic_with_error!(&env, Error::WrongMode);
        }
        if proceeds < 0 || returned < 0 || returned > b.units {
            panic_with_error!(&env, Error::InvalidAmount);
        }
        let s = Self::settings(env.clone());
        let price = if b.strike != 0 {
            b.strike
        } else {
            Self::nav(&env, &s)
        };
        if price == 0 {
            panic_with_error!(&env, Error::NoPrice);
        }
        let worth = mul_div_ceil(&env, b.units - returned, price, ONE);
        let floor = worth.and_then(|w| mul_div_ceil(&env, w, BPS - s.floor_bps as i128, BPS));
        if proceeds < floor.unwrap_or_else(|_| panic_with_error!(&env, Error::InvalidAmount)) {
            panic_with_error!(&env, Error::ProceedsTooLow);
        }
        let init = Self::init(env.clone());
        let me = env.current_contract_address();
        token::Client::new(&env, &init.stable).transfer(&operator, &me, &proceeds);
        if returned != 0 {
            token::Client::new(&env, &init.rwa).transfer(&operator, &me, &returned);
        }
        b.unsent = returned;
        Self::finish(&env, id, b, proceeds);
    }

    // ---- accounts

    /// Admin: adds `n` redemption accounts, at the addresses `account_address` reports.
    pub fn add_accounts(env: Env, n: u32) {
        Self::roles(env.clone()).admin.require_auth();
        for _ in 0..n {
            Self::add_account(&env);
        }
    }

    /// Where account `index` is or will be deployed, to whitelist it with the issuer ahead of time.
    pub fn account_address(env: Env, index: u32) -> Address {
        env.deployer()
            .with_current_contract(Self::salt(&env, ACCOUNT_SALT, index as u64))
            .deployed_address()
    }

    pub fn accounts(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&Key::Accounts)
            .unwrap_or(Vec::new(&env))
    }

    // ---- governance

    /// Admin: queues `change`; it can be accepted once the delay has passed.
    pub fn submit(env: Env, change: Change) -> u64 {
        Self::roles(env.clone()).admin.require_auth();
        if let Change::Settings(s) = &change {
            Self::check_settings(&env, s);
        }
        timelock::submit(&env, Self::kind(&change), &change)
    }

    /// Anyone applies a change once its delay has passed.
    pub fn accept(env: Env, kind: u32) {
        match timelock::take::<Change>(&env, kind) {
            Change::Settings(s) => Self::apply_settings(&env, s),
            Change::Roles(r) => {
                timelock::init(&env, timelock::delay(&env), &r.guardian);
                env.storage().instance().set(&Key::Roles, &r);
            }
            Change::Skim(token, to) => Self::skim(&env, &token, &to),
            Change::Delay(d) => timelock::init(&env, d, &timelock::guardian(&env)),
        }
    }

    pub fn pending(env: Env, kind: u32) -> Option<(Change, u64)> {
        timelock::pending(&env, kind)
    }

    pub fn revoke(env: Env, kind: u32, caller: Address) {
        timelock::revoke(&env, kind, &caller, &Self::roles(env.clone()).admin);
    }

    /// The guardian or the admin pauses new requests. Nothing else stops.
    pub fn set_paused(env: Env, caller: Address, paused: bool) {
        caller.require_auth();
        let r = Self::roles(env.clone());
        if caller != r.admin && caller != r.guardian {
            panic_with_error!(&env, Error::NotAuthorized);
        }
        env.storage().instance().set(&Key::Paused, &paused);
    }

    // ---- views

    pub fn init(env: Env) -> Init {
        env.storage().instance().get(&Key::Init).unwrap()
    }

    pub fn roles(env: Env) -> Roles {
        env.storage().instance().get(&Key::Roles).unwrap()
    }

    pub fn settings(env: Env) -> Settings {
        env.storage().instance().get(&Key::Settings).unwrap()
    }

    pub fn totals(env: Env) -> Totals {
        env.storage()
            .instance()
            .get(&Key::Totals)
            .unwrap_or_default()
    }

    pub fn paused(env: Env) -> bool {
        env.storage().instance().get(&Key::Paused).unwrap_or(false)
    }

    pub fn batch(env: Env, id: u64) -> Option<Batch> {
        env.storage().persistent().get(&Key::Batch(id))
    }

    pub fn ct_of(env: Env, id: u64) -> Address {
        Self::batch_of(&env, id).ct
    }

    pub fn open_batch(env: Env) -> Option<u64> {
        env.storage().instance().get(&Key::Open)
    }

    /// What one CT of batch `id` is worth in the stable, 1e18-scaled, net of
    /// the fee: its cash share once settled, else its strike, else `price`.
    pub fn claim_value(env: Env, id: u64, price: i128) -> i128 {
        let b = Self::batch_of(&env, id);
        if b.status == Status::Settled {
            let supply = ClaimTokenClient::new(&env, &b.ct).total_supply();
            if supply == 0 {
                return 0;
            }
            return Self::md(&env, b.remaining, ONE, supply)
                + Self::md(&env, b.unsent, price, supply);
        }
        let p = if b.strike != 0 { b.strike } else { price };
        let fee = Self::settings(env.clone()).fee_bps as i128;
        Self::md(&env, p, BPS - fee, BPS)
    }

    /// An oracle reading for one of this vault's CTs in its stable, at the
    /// RWA's live NAV: the vault serves as the price source for its own CTs.
    pub fn get_price(env: Env, base: Address, quote: Address) -> PriceData {
        let id: Option<u64> = env.storage().persistent().get(&Key::Ct(base));
        let price = match id {
            Some(id) if quote == Self::init(env.clone()).stable => {
                let nav = Self::nav(&env, &Self::settings(env.clone()));
                Self::claim_value(env.clone(), id, nav)
            }
            _ => 0,
        };
        PriceData {
            price,
            timestamp: env.ledger().timestamp(),
        }
    }

    // ---- internals

    /// The open batch, opening one (and deploying its CT) if there is none.
    fn open(env: &Env) -> (u64, Batch) {
        if let Some(id) = Self::open_batch(env.clone()) {
            return (id, Self::batch_of(env, id));
        }
        let id: u64 = env.storage().instance().get(&Key::Next).unwrap();
        env.storage().instance().set(&Key::Next, &(id + 1));
        env.storage().instance().set(&Key::Open, &id);
        let ct = Self::deploy_ct(env, id);
        Self::put(env, &Key::Ct(ct.clone()), &id);
        let b = Batch {
            status: Status::Open,
            ct,
            account: None,
            units: 0,
            unsent: 0,
            proceeds: 0,
            fee: 0,
            remaining: 0,
            strike: 0,
            nav_start: 0,
            nav_end: 0,
            sent_at: 0,
            settled_at: 0,
        };
        (id, b)
    }

    /// "Octarine Claim <RWA> #<id>", symbol "ct<RWA>-<id>".
    fn deploy_ct(env: &Env, id: u64) -> Address {
        let init = Self::init(env.clone());
        let mut sym = [0u8; 24];
        let rwa_sym = token::Client::new(env, &init.rwa).symbol();
        let n = rwa_sym.len() as usize;
        let sym = if n <= sym.len() {
            rwa_sym.copy_into_slice(&mut sym[..n]);
            &sym[..n]
        } else {
            b"RWA".as_slice()
        };
        let mut digits = [0u8; 20];
        let d = Self::digits(id, &mut digits);
        let name = Self::concat(env, [b"Octarine Claim ", sym, b" #", d]);
        let symbol = Self::concat(env, [b"ct", sym, b"-", d]);
        let args = (
            env.current_contract_address(),
            init.rwa,
            id,
            Self::settings(env.clone()).gate,
            name,
            symbol,
        );
        env.deployer()
            .with_current_contract(Self::salt(env, CT_SALT, id))
            .deploy_v2(init.ct_wasm, args)
    }

    fn digits(mut v: u64, buf: &mut [u8; 20]) -> &[u8] {
        let mut i = buf.len();
        loop {
            i -= 1;
            buf[i] = b'0' + (v % 10) as u8;
            v /= 10;
            if v == 0 {
                return &buf[i..];
            }
        }
    }

    fn concat(env: &Env, parts: [&[u8]; 4]) -> String {
        let mut buf = [0u8; 64];
        let mut n = 0;
        for p in parts {
            buf[n..n + p.len()].copy_from_slice(p);
            n += p.len();
        }
        String::from_bytes(env, &buf[..n])
    }

    fn salt(env: &Env, tag: u8, n: u64) -> BytesN<32> {
        let mut salt = [0u8; 32];
        salt[0] = tag;
        salt[24..].copy_from_slice(&n.to_be_bytes());
        BytesN::from_array(env, &salt)
    }

    fn add_account(env: &Env) {
        let mut accounts = Self::accounts(env.clone());
        let account = env
            .deployer()
            .with_current_contract(Self::salt(env, ACCOUNT_SALT, accounts.len() as u64))
            .deploy_v2(
                Self::init(env.clone()).account_wasm,
                (env.current_contract_address(),),
            );
        accounts.push_back(account.clone());
        env.storage().instance().set(&Key::Accounts, &accounts);
        env.events().publish((symbol_short!("account"),), account);
    }

    fn free_account(env: &Env) -> Address {
        Self::accounts(env.clone())
            .iter()
            .find(|a| !env.storage().persistent().has(&Key::Busy(a.clone())))
            .unwrap_or_else(|| panic_with_error!(env, Error::NoFreeAccount))
    }

    fn resend(env: &Env, id: u64, mut b: Batch, spender: Option<Address>, calls: Vec<Call>) {
        if b.unsent == 0 {
            panic_with_error!(env, Error::InvalidAmount);
        }
        let account = b
            .account
            .clone()
            .unwrap_or_else(|| panic_with_error!(env, Error::WrongMode));
        Self::send_from(env, &account, b.unsent, spender, &calls);
        env.events().publish((symbol_short!("sent"), id), b.unsent);
        b.unsent = 0;
        Self::put(env, &Key::Batch(id), &b);
    }

    /// Runs `calls` from `account`, which must spend exactly `amount` of its RWA.
    fn send_from(
        env: &Env,
        account: &Address,
        amount: i128,
        spender: Option<Address>,
        calls: &Vec<Call>,
    ) {
        let rwa = Self::init(env.clone()).rwa;
        let acct = AccountClient::new(env, account);
        let before = token::Client::new(env, &rwa).balance(account);
        if let Some(sp) = &spender {
            acct.approve(&rwa, sp, &amount);
        }
        Self::run_calls(env, account, calls);
        if let Some(sp) = &spender {
            acct.approve(&rwa, sp, &0);
        }
        if before - token::Client::new(env, &rwa).balance(account) != amount {
            panic_with_error!(env, Error::IssuerTookWrongAmount);
        }
    }

    /// Each call runs from `account`, never the vault, and only as the
    /// verifier allows; never against the stable, the account or a CT.
    fn run_calls(env: &Env, account: &Address, calls: &Vec<Call>) {
        let me = env.current_contract_address();
        let stable = Self::init(env.clone()).stable;
        let verifier = Self::settings(env.clone())
            .verifier
            .unwrap_or_else(|| panic_with_error!(env, Error::CallNotVerified));
        let v = CallVerifierClient::new(env, &verifier);
        let acct = AccountClient::new(env, account);
        for c in calls.iter() {
            let forbidden = c.target == stable
                || &c.target == account
                || c.target == me
                || env.storage().persistent().has(&Key::Ct(c.target.clone()));
            if forbidden || !v.verify(&me, account, &c.target, &c.func, &c.args) {
                panic_with_error!(env, Error::CallNotVerified);
            }
            acct.execute(&c.target, &c.func, &c.args);
        }
    }

    /// Records an open batch as with the issuer, struck now if strikes are at send.
    fn mark_sent(env: &Env, id: u64, mut b: Batch, s: &Settings) {
        if b.units <= 0 {
            panic_with_error!(env, Error::InvalidAmount);
        }
        b.nav_start = Self::nav(env, s);
        if s.strike_at_send {
            if b.nav_start == 0 {
                panic_with_error!(env, Error::NoPrice);
            }
            b.strike = b.nav_start;
        }
        if Self::open_batch(env.clone()) == Some(id) {
            env.storage().instance().remove(&Key::Open);
        }
        b.status = Status::Sent;
        b.sent_at = env.ledger().timestamp();
        Self::tally(env, |t| t.sent += 1);
        env.events()
            .publish((symbol_short!("sent"), id), (b.units, b.strike));
        Self::put(env, &Key::Batch(id), &b);
    }

    fn finish(env: &Env, id: u64, mut b: Batch, proceeds: i128) {
        let s = Self::settings(env.clone());
        let fee = Self::md(env, proceeds, s.fee_bps as i128, BPS);
        Self::tally(env, |t| {
            t.proceeds += proceeds - fee;
            t.rwa += b.unsent;
            t.sent -= 1;
        });
        b.status = Status::Settled;
        b.proceeds = proceeds;
        b.fee = fee;
        b.remaining = proceeds - fee;
        b.nav_end = Self::nav(env, &s);
        b.settled_at = env.ledger().timestamp();
        Self::put(env, &Key::Batch(id), &b);
        if let Some(to) = s.fee_recipient.filter(|_| fee > 0) {
            Self::send(env, &Self::init(env.clone()).stable, &to, fee);
        }
        env.events()
            .publish((symbol_short!("settled"), id), (proceeds, fee, b.unsent));
    }

    /// The RWA's NAV in the stable, 1e18-scaled; 0 without a fresh reading.
    fn nav(env: &Env, s: &Settings) -> i128 {
        let Some(oracle) = &s.oracle else { return 0 };
        let init = Self::init(env.clone());
        match OracleClient::new(env, oracle).try_get_price(&init.rwa, &init.stable) {
            Ok(Ok(p)) if p.price > 0 && env.ledger().timestamp() <= p.timestamp + s.max_age => {
                p.price
            }
            _ => 0,
        }
    }

    fn check_settings(env: &Env, s: &Settings) {
        let bad = s.fee_bps > MAX_FEE_BPS
            || (s.fee_bps > 0 && s.fee_recipient.is_none())
            || s.floor_bps as i128 > BPS;
        if bad {
            panic_with_error!(env, Error::InvalidSettings);
        }
    }

    fn apply_settings(env: &Env, s: Settings) {
        let old = Self::settings(env.clone());
        let switched = s.mode != old.mode || s.strike_at_send != old.strike_at_send;
        if switched && Self::totals(env.clone()).sent != 0 {
            panic_with_error!(env, Error::BatchesInFlight);
        }
        if s.gate != old.gate {
            if let Some(id) = Self::open_batch(env.clone()) {
                ClaimTokenClient::new(env, &Self::batch_of(env, id).ct).set_gate(&s.gate);
            }
        }
        env.storage().instance().set(&Key::Settings, &s);
    }

    fn skim(env: &Env, token: &Address, to: &Address) {
        let init = Self::init(env.clone());
        let t = Self::totals(env.clone());
        let mut amount = token::Client::new(env, token).balance(&env.current_contract_address());
        if token == &init.stable {
            amount -= t.proceeds;
        }
        if token == &init.rwa {
            amount -= t.pooled + t.rwa;
        }
        Self::send(env, token, to, amount);
    }

    fn kind(c: &Change) -> u32 {
        match c {
            Change::Settings(_) => 0,
            Change::Roles(_) => 1,
            Change::Skim(..) => 2,
            Change::Delay(_) => 3,
        }
    }

    fn mode(env: &Env, mode: Mode) -> Settings {
        let s = Self::settings(env.clone());
        if s.mode != mode {
            panic_with_error!(env, Error::WrongMode);
        }
        s
    }

    fn batch_of(env: &Env, id: u64) -> Batch {
        Self::batch(env.clone(), id).unwrap_or_else(|| panic_with_error!(env, Error::WrongStatus))
    }

    fn with_status(env: &Env, id: u64, status: Status) -> Batch {
        let b = Self::batch_of(env, id);
        if b.status != status {
            panic_with_error!(env, Error::WrongStatus);
        }
        b
    }

    fn tally(env: &Env, f: impl FnOnce(&mut Totals)) {
        let mut t = Self::totals(env.clone());
        f(&mut t);
        env.storage().instance().set(&Key::Totals, &t);
    }

    fn send(env: &Env, token: &Address, to: &Address, amount: i128) {
        if amount > 0 {
            token::Client::new(env, token).transfer(&env.current_contract_address(), to, &amount);
        }
    }

    fn md(env: &Env, a: i128, b: i128, d: i128) -> i128 {
        mul_div(env, a, b, d).unwrap_or_else(|_| panic_with_error!(env, Error::InvalidAmount))
    }

    fn put<V: IntoVal<Env, Val>>(env: &Env, k: &Key, v: &V) {
        env.storage().persistent().set(k, v);
        env.storage().persistent().extend_ttl(k, THRESHOLD, EXTEND);
        env.storage().instance().extend_ttl(THRESHOLD, EXTEND);
    }
}
