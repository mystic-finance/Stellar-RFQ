#![no_std]
//! Octarine DEX aggregator: a standalone swap router that collects its own
//! fees, and the RFQ router's `Leg::Dex` source. Routes are computed off-chain
//! and executed as given, either through allowlisted DEX adapters (`swap`) or,
//! SimpleSwap-style, as calldata against allowlisted routers (`swap_calls`).
//! See docs/FACILITY_V2_STELLAR.md.

mod errors;
mod types;

#[cfg(test)]
mod invariant;
#[cfg(test)]
mod test;
#[cfg(test)]
mod test_calls;
#[cfg(test)]
mod test_e2e;

use common::interfaces::{Aggregator, DexAdapterClient};
use common::math::{self, BPS};
use soroban_sdk::auth::InvokerContractAuthEntry;
use soroban_sdk::xdr::FromXdr;
use soroban_sdk::{
    contract, contractimpl, contracttype, panic_with_error, symbol_short, token, Address, Bytes,
    BytesN, Env, Symbol, TryFromVal, Val, Vec,
};

pub use errors::Error;
pub use types::{Call, FeeConfig, Hop, Partner, Route, Split, SwapDescription, SwapResult};

const MAX_PROTOCOL_FEE_BPS: u32 = 100;
const MAX_PARTNER_FEE_BPS: u32 = 300;
const MAX_VENUES: u32 = 16;
const MAX_SPLITS: u32 = 8;
const MAX_HOPS: u32 = 4;
const THRESHOLD: u32 = 518_400;
const EXTEND: u32 = 535_680;

#[contracttype]
#[derive(Clone)]
enum Key {
    Admin,
    Router,
    Fees,
    Paused,
    Venues,
    Targets,
    Owed(Address, Address),
    TotalOwed(Address),
}

#[contract]
pub struct DexAggregator;

#[contractimpl]
impl DexAggregator {
    pub fn __constructor(env: Env, admin: Address, router: Address, fees: FeeConfig) {
        Self::check_fees(&env, &fees);
        let s = env.storage().instance();
        s.set(&Key::Admin, &admin);
        s.set(&Key::Router, &router);
        s.set(&Key::Fees, &fees);
        s.set(&Key::Venues, &Vec::<Address>::new(&env));
        Self::touch(&env);
    }

    pub fn allow_venue(env: Env, venue: Address, allowed: bool) {
        Self::admin_auth(&env);
        let mut list = Self::venues(env.clone());
        match (list.first_index_of(&venue), allowed) {
            (None, true) => {
                if list.len() >= MAX_VENUES {
                    panic_with_error!(&env, Error::TooManyVenues);
                }
                list.push_back(venue.clone());
            }
            (Some(i), false) => {
                list.remove(i);
            }
            _ => return,
        }
        env.storage().instance().set(&Key::Venues, &list);
        env.events()
            .publish((symbol_short!("venue"), venue), allowed);
    }

    pub fn allow_target(env: Env, target: Address, allowed: bool) {
        Self::admin_auth(&env);
        let mut list = Self::targets(env.clone());
        match (list.first_index_of(&target), allowed) {
            (None, true) => {
                if list.len() >= MAX_VENUES {
                    panic_with_error!(&env, Error::TooManyVenues);
                }
                list.push_back(target.clone());
            }
            (Some(i), false) => {
                list.remove(i);
            }
            _ => return,
        }
        env.storage().instance().set(&Key::Targets, &list);
        env.events()
            .publish((symbol_short!("target"), target), allowed);
    }

    pub fn set_fees(env: Env, fees: FeeConfig) {
        Self::admin_auth(&env);
        Self::check_fees(&env, &fees);
        env.storage().instance().set(&Key::Fees, &fees);
    }

    pub fn set_router(env: Env, router: Address) {
        Self::admin_auth(&env);
        env.storage().instance().set(&Key::Router, &router);
    }

    /// Stops swaps. Never stops anyone claiming fees they are owed.
    pub fn set_paused(env: Env, paused: bool) {
        Self::admin_auth(&env);
        env.storage().instance().set(&Key::Paused, &paused);
    }

    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        Self::admin_auth(&env);
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    pub fn admin(env: Env) -> Address {
        env.storage().instance().get(&Key::Admin).unwrap()
    }

    pub fn router(env: Env) -> Address {
        env.storage().instance().get(&Key::Router).unwrap()
    }

    pub fn fees(env: Env) -> FeeConfig {
        env.storage().instance().get(&Key::Fees).unwrap()
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage().instance().get(&Key::Paused).unwrap_or(false)
    }

    /// Routers that calldata routes may call.
    pub fn targets(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&Key::Targets)
            .unwrap_or(Vec::new(&env))
    }

    pub fn venues(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&Key::Venues)
            .unwrap_or(Vec::new(&env))
    }

    pub fn owed(env: Env, recipient: Address, token: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&Key::Owed(recipient, token))
            .unwrap_or(0)
    }

    /// Fees accrued in `token` and not yet claimed. The aggregator's balance of
    /// `token` never falls below this.
    pub fn total_owed(env: Env, token: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&Key::TotalOwed(token))
            .unwrap_or(0)
    }

    /// Simulates `route` exactly, net of fees: what the backend shows a user.
    pub fn quote_route(
        env: Env,
        token_in: Address,
        token_out: Address,
        amount_in: i128,
        route: Vec<Split>,
        partner: Option<Partner>,
    ) -> i128 {
        Self::validate(&env, &token_in, &token_out, amount_in, &route);
        let mut gross = 0;
        for split in route.iter() {
            let (mut amount, mut tok) = (split.amount_in, token_in.clone());
            for hop in split.hops.iter() {
                amount = DexAdapterClient::new(&env, &hop.venue).quote(
                    &tok,
                    &hop.token_out,
                    &amount,
                    &hop.data,
                );
                tok = hop.token_out;
            }
            gross += amount;
        }
        let (protocol, partner_fee) = Self::fees_on(&env, gross, &partner);
        gross - protocol - partner_fee
    }

    /// A direct swap. `user` authorises and pays; `desc.to` receives.
    pub fn swap(
        env: Env,
        user: Address,
        desc: SwapDescription,
        route: Vec<Split>,
        partner: Option<Partner>,
    ) -> SwapResult {
        user.require_auth();
        Self::not_paused(&env);
        Self::validate(
            &env,
            &desc.token_in,
            &desc.token_out,
            desc.amount_in,
            &route,
        );
        let me = env.current_contract_address();
        let before = Self::free_balance(&env, &desc.token_in);
        token::Client::new(&env, &desc.token_in).transfer(&user, &me, &desc.amount_in);
        if Self::free_balance(&env, &desc.token_in) - before < desc.amount_in {
            panic_with_error!(&env, Error::InputShortfall);
        }
        let gross = Self::execute(&env, &desc.token_in, &route);
        Self::settle(
            &env,
            &desc.token_out,
            gross,
            &partner,
            desc.min_out,
            &desc.to,
        )
    }

    /// A swap along calldata built off-chain. Unspent input goes back to `user`.
    pub fn swap_calls(
        env: Env,
        user: Address,
        desc: SwapDescription,
        calls: Vec<Call>,
        partner: Option<Partner>,
    ) -> SwapResult {
        user.require_auth();
        Self::not_paused(&env);
        if desc.token_in == desc.token_out || desc.amount_in <= 0 || calls.is_empty() {
            panic_with_error!(&env, Error::InvalidRoute);
        }
        let me = env.current_contract_address();
        let before = Self::free_balance(&env, &desc.token_in);
        token::Client::new(&env, &desc.token_in).transfer(&user, &me, &desc.amount_in);
        if Self::free_balance(&env, &desc.token_in) - before < desc.amount_in {
            panic_with_error!(&env, Error::InputShortfall);
        }
        let gross = Self::execute_calls(
            &env,
            &desc.token_in,
            &desc.token_out,
            desc.amount_in,
            &calls,
            &user,
        );
        Self::settle(
            &env,
            &desc.token_out,
            gross,
            &partner,
            desc.min_out,
            &desc.to,
        )
    }

    /// Pays out everything `recipient` is owed in each of `tokens`.
    pub fn claim(env: Env, recipient: Address, tokens: Vec<Address>) -> Vec<i128> {
        recipient.require_auth();
        let me = env.current_contract_address();
        let mut paid = Vec::new(&env);
        for token in tokens.iter() {
            let amount = Self::owed(env.clone(), recipient.clone(), token.clone());
            if amount > 0 {
                Self::credit(&env, &recipient, &token, -amount);
                token::Client::new(&env, &token).transfer(&me, &recipient, &amount);
                env.events()
                    .publish((symbol_short!("claim"), recipient.clone(), token), amount);
            }
            paid.push_back(amount);
        }
        paid
    }

    fn validate(
        env: &Env,
        token_in: &Address,
        token_out: &Address,
        amount_in: i128,
        route: &Vec<Split>,
    ) {
        if token_in == token_out || amount_in <= 0 || route.is_empty() || route.len() > MAX_SPLITS {
            panic_with_error!(env, Error::InvalidRoute);
        }
        let venues = Self::venues(env.clone());
        let mut total = 0;
        for split in route.iter() {
            Self::validate_split(env, &venues, token_out, &split);
            total += split.amount_in;
        }
        if total != amount_in {
            panic_with_error!(env, Error::InvalidRoute);
        }
    }

    fn validate_split(env: &Env, venues: &Vec<Address>, token_out: &Address, split: &Split) {
        let ends_right = split.hops.last().is_some_and(|h| &h.token_out == token_out);
        if split.amount_in <= 0 || split.hops.len() > MAX_HOPS || !ends_right {
            panic_with_error!(env, Error::InvalidRoute);
        }
        for hop in split.hops.iter() {
            if venues.first_index_of(&hop.venue).is_none() {
                panic_with_error!(env, Error::VenueNotAllowed);
            }
        }
    }

    /// Runs every split; returns the total delivered, each hop measured at the
    /// aggregator rather than taken from what the venue reports.
    fn execute(env: &Env, token_in: &Address, route: &Vec<Split>) -> i128 {
        let me = env.current_contract_address();
        let mut gross = 0;
        for split in route.iter() {
            let (mut amount, mut tok) = (split.amount_in, token_in.clone());
            for hop in split.hops.iter() {
                let out = token::Client::new(env, &hop.token_out);
                token::Client::new(env, &tok).transfer(&me, &hop.venue, &amount);
                let before = out.balance(&me);
                DexAdapterClient::new(env, &hop.venue).swap(
                    &tok,
                    &hop.token_out,
                    &amount,
                    &0,
                    &hop.data,
                    &me,
                );
                amount = out.balance(&me) - before;
                tok = hop.token_out;
            }
            gross += amount;
        }
        gross
    }

    /// Runs each call against an allowlisted router. A call may only spend
    /// `token_in`, by transfers it pre-authorises, `amount_in` in all; the
    /// output is measured here, and unspent input returns to `refund`.
    fn execute_calls(
        env: &Env,
        token_in: &Address,
        token_out: &Address,
        amount_in: i128,
        calls: &Vec<Call>,
        refund: &Address,
    ) -> i128 {
        let me = env.current_contract_address();
        let (input, output) = (
            token::Client::new(env, token_in),
            token::Client::new(env, token_out),
        );
        let (in_before, out_before) = (input.balance(&me), output.balance(&me));
        let targets = Self::targets(env.clone());
        let mut budget = amount_in;
        for call in calls.iter() {
            if targets.first_index_of(&call.target).is_none()
                || &call.target == token_in
                || &call.target == token_out
            {
                panic_with_error!(env, Error::VenueNotAllowed);
            }
            for entry in call.auth.iter() {
                budget -= Self::spend_of(env, token_in, &me, &entry);
            }
            if budget < 0 {
                panic_with_error!(env, Error::InvalidRoute);
            }
            if !call.auth.is_empty() {
                env.authorize_as_current_contract(call.auth.clone());
            }
            env.invoke_contract::<Val>(&call.target, &call.function, call.args.clone());
        }
        let spent = in_before - input.balance(&me);
        if spent > amount_in {
            panic_with_error!(env, Error::InputShortfall);
        }
        if spent < amount_in {
            input.transfer(&me, refund, &(amount_in - spent));
        }
        output.balance(&me) - out_before
    }

    /// The amount an auth entry lets a router take: only `token_in.transfer`
    /// from this contract, with nothing nested beneath it.
    fn spend_of(
        env: &Env,
        token_in: &Address,
        me: &Address,
        entry: &InvokerContractAuthEntry,
    ) -> i128 {
        if let InvokerContractAuthEntry::Contract(sub) = entry {
            let c = &sub.context;
            let from = c
                .args
                .get(0)
                .and_then(|v| Address::try_from_val(env, &v).ok());
            let amount = c.args.get(2).and_then(|v| i128::try_from_val(env, &v).ok());
            if &c.contract == token_in
                && c.fn_name == Symbol::new(env, "transfer")
                && sub.sub_invocations.is_empty()
                && from.as_ref() == Some(me)
            {
                if let Some(a) = amount.filter(|a| *a >= 0) {
                    return a;
                }
            }
        }
        panic_with_error!(env, Error::InvalidRoute)
    }

    /// Accrues fees, pays the rest to `to`, and holds it to `min_out`.
    fn settle(
        env: &Env,
        token_out: &Address,
        gross: i128,
        partner: &Option<Partner>,
        min_out: i128,
        to: &Address,
    ) -> SwapResult {
        let (protocol_fee, partner_fee) = Self::fees_on(env, gross, partner);
        let amount_out = gross - protocol_fee - partner_fee;
        if amount_out < min_out {
            panic_with_error!(env, Error::BelowMinOut);
        }
        Self::accrue_fees(env, token_out, protocol_fee, partner_fee, partner);
        token::Client::new(env, token_out).transfer(
            &env.current_contract_address(),
            to,
            &amount_out,
        );
        env.events().publish(
            (symbol_short!("swap"), to.clone(), token_out.clone()),
            (gross, protocol_fee, partner_fee),
        );
        SwapResult {
            gross_out: gross,
            protocol_fee,
            partner_fee,
            amount_out,
        }
    }

    fn fees_on(env: &Env, gross: i128, partner: &Option<Partner>) -> (i128, i128) {
        let fees = Self::fees(env.clone());
        let protocol = Self::bps(env, gross, fees.protocol_fee_bps);
        let partner_fee = match partner {
            Some(p) if p.fee_bps > fees.max_partner_fee_bps => {
                panic_with_error!(env, Error::PartnerFeeTooHigh)
            }
            Some(p) => Self::bps(env, gross, p.fee_bps),
            None => 0,
        };
        (protocol, partner_fee)
    }

    /// Fees accrue rather than being pushed: a transfer to an account without a
    /// trustline fails, so pushing would let one misconfigured partner address
    /// break every swap that partner integrates.
    fn accrue_fees(
        env: &Env,
        token: &Address,
        protocol_fee: i128,
        partner_fee: i128,
        partner: &Option<Partner>,
    ) {
        let fees = Self::fees(env.clone());
        let mut to_protocol = protocol_fee;
        if let Some(p) = partner {
            let cut = Self::bps(env, partner_fee, fees.partner_share_bps);
            to_protocol += cut;
            Self::credit(env, &p.address, token, partner_fee - cut);
        }
        Self::credit(env, &fees.recipient, token, to_protocol);
    }

    fn credit(env: &Env, recipient: &Address, token: &Address, amount: i128) {
        if amount == 0 {
            return;
        }
        let owed = Key::Owed(recipient.clone(), token.clone());
        let total = Key::TotalOwed(token.clone());
        let p = env.storage().persistent();
        p.set(&owed, &(p.get::<_, i128>(&owed).unwrap_or(0) + amount));
        p.set(&total, &(p.get::<_, i128>(&total).unwrap_or(0) + amount));
        p.extend_ttl(&owed, THRESHOLD, EXTEND);
        p.extend_ttl(&total, THRESHOLD, EXTEND);
    }

    /// Balance not already owed to a fee recipient.
    fn free_balance(env: &Env, token: &Address) -> i128 {
        token::Client::new(env, token).balance(&env.current_contract_address())
            - Self::total_owed(env.clone(), token.clone())
    }

    fn bps(env: &Env, amount: i128, bps: u32) -> i128 {
        match math::mul_div(env, amount, bps as i128, BPS) {
            Ok(v) => v,
            Err(_) => panic_with_error!(env, Error::Overflow),
        }
    }

    fn check_fees(env: &Env, f: &FeeConfig) {
        if f.protocol_fee_bps > MAX_PROTOCOL_FEE_BPS
            || f.max_partner_fee_bps > MAX_PARTNER_FEE_BPS
            || f.partner_share_bps > BPS as u32
        {
            panic_with_error!(env, Error::InvalidConfig);
        }
    }

    fn not_paused(env: &Env) {
        if Self::is_paused(env.clone()) {
            panic_with_error!(env, Error::Paused);
        }
    }

    fn admin_auth(env: &Env) {
        Self::admin(env.clone()).require_auth();
        Self::touch(env);
    }

    fn touch(env: &Env) {
        env.storage().instance().extend_ttl(THRESHOLD, EXTEND);
    }
}

#[contractimpl]
impl Aggregator for DexAggregator {
    /// The best direct single-hop quote across allowlisted venues, net of the
    /// protocol fee. Advisory: real routes are priced with `quote_route`.
    fn quote(env: Env, token_in: Address, token_out: Address, amount_in: i128) -> i128 {
        let mut best = 0;
        for venue in Self::venues(env.clone()).iter() {
            if let Ok(Ok(q)) = DexAdapterClient::new(&env, &venue).try_quote(
                &token_in,
                &token_out,
                &amount_in,
                &Bytes::new(&env),
            ) {
                best = best.max(q);
            }
        }
        best - Self::fees_on(&env, best, &None).0
    }

    /// Router only. The router has already transferred `amount_in`; `data` is
    /// a `Route` as XDR. No partner fee in this path.
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
        Self::not_paused(&env);
        if data.is_empty() {
            panic_with_error!(&env, Error::InvalidRoute);
        }
        if Self::free_balance(&env, &token_in) < amount_in {
            panic_with_error!(&env, Error::InputShortfall);
        }
        let gross = match Route::from_xdr(&env, &data) {
            Ok(Route::Splits(route)) => {
                Self::validate(&env, &token_in, &token_out, amount_in, &route);
                Self::execute(&env, &token_in, &route)
            }
            Ok(Route::Calls(calls)) => {
                Self::execute_calls(&env, &token_in, &token_out, amount_in, &calls, &recipient)
            }
            Err(_) => panic_with_error!(&env, Error::InvalidRoute),
        };
        Self::settle(&env, &token_out, gross, &None, min_amount_out, &recipient).amount_out
    }
}
