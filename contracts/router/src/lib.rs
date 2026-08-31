#![no_std]
//! Octarine RFQ router: settles the route the taker chose across signed LP bids
//! and whitelisted aggregators. Ranking and route choice happen off-chain; this
//! contract executes the decision and takes no fee of its own. See README.md.

mod errors;
mod types;

#[cfg(test)]
mod test;

use soroban_sdk::{
    contract, contractimpl, panic_with_error, symbol_short, token, Address, BytesN, Env, Vec,
};

pub use errors::Error;
pub use types::{
    Aggregator, AggregatorClient, AggregatorLeg, FillResult, FixedOrder, Leg, RfqLeg, RfqOrder,
    RouteResult, Settlement, SettlementClient, Signature, SignedOrder, SourceEntry, SourceKind,
    SourceQuote,
};

use soroban_sdk::contracttype;

#[contracttype]
pub enum DataKey {
    Admin,
    Settlement,
    Paused,
    Sources,
}

const THRESHOLD: u32 = 518_400;
const EXTEND: u32 = 535_680;

struct Opening {
    held_in: i128,
    held_out: i128,
    taker_in: i128,
    taker_out: i128,
}

#[contract]
pub struct RouterContract;

#[contractimpl]
impl RouterContract {
    pub fn initialize(env: Env, admin: Address, settlement: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&env, Error::AlreadyInitialized);
        }
        // Deploy and initialize are not necessarily one transaction.
        admin.require_auth();
        let store = env.storage().instance();
        store.set(&DataKey::Admin, &admin);
        store.set(&DataKey::Settlement, &settlement);
        store.set(&DataKey::Sources, &Vec::<SourceEntry>::new(&env));
        Self::touch(&env);
    }

    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        Self::admin(env.clone()).require_auth();
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    pub fn register_source(env: Env, source: Address, kind: SourceKind, allowed: bool) {
        Self::admin(env.clone()).require_auth();
        let mut sources = Self::sources(env.clone());
        let found = Self::index_of(&sources, &source);
        match (found, allowed) {
            (None, true) => sources.push_back(SourceEntry {
                address: source.clone(),
                kind,
            }),
            (Some(i), true) => sources.set(
                i,
                SourceEntry {
                    address: source.clone(),
                    kind,
                },
            ),
            (Some(i), false) => {
                sources.remove(i);
            }
            _ => return,
        }
        env.storage().instance().set(&DataKey::Sources, &sources);
        Self::touch(&env);
        env.events()
            .publish((symbol_short!("source"), source), (kind, allowed));
    }

    pub fn set_settlement(env: Env, settlement: Address) {
        Self::admin(env.clone()).require_auth();
        env.storage()
            .instance()
            .set(&DataKey::Settlement, &settlement);
        Self::touch(&env);
    }

    pub fn set_paused(env: Env, paused: bool) {
        Self::admin(env.clone()).require_auth();
        env.storage().instance().set(&DataKey::Paused, &paused);
        Self::touch(&env);
    }

    pub fn admin(env: Env) -> Address {
        env.storage().instance().get(&DataKey::Admin).unwrap()
    }

    pub fn settlement(env: Env) -> Address {
        env.storage().instance().get(&DataKey::Settlement).unwrap()
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    pub fn sources(env: Env) -> Vec<SourceEntry> {
        env.storage()
            .instance()
            .get(&DataKey::Sources)
            .unwrap_or(Vec::new(&env))
    }

    pub fn quote(
        env: Env,
        taker_token: Address,
        maker_token: Address,
        taker_amount: i128,
    ) -> Vec<SourceQuote> {
        let mut out = Vec::new(&env);
        for entry in Self::sources(env.clone()).iter() {
            if let Some(maker_amount) = Self::try_quote(
                &env,
                &entry.address,
                &taker_token,
                &maker_token,
                taker_amount,
            ) {
                out.push_back(SourceQuote {
                    source: entry.address,
                    kind: entry.kind,
                    maker_amount,
                });
            }
        }
        out
    }

    pub fn best_quote(
        env: Env,
        taker_token: Address,
        maker_token: Address,
        taker_amount: i128,
    ) -> Option<SourceQuote> {
        let mut best: Option<SourceQuote> = None;
        for q in Self::quote(env, taker_token, maker_token, taker_amount).iter() {
            if best
                .as_ref()
                .is_none_or(|b| q.maker_amount > b.maker_amount)
            {
                best = Some(q);
            }
        }
        best
    }

    pub fn fill(
        env: Env,
        taker: Address,
        taker_token: Address,
        maker_token: Address,
        route: Vec<Leg>,
        min_out: i128,
    ) -> RouteResult {
        taker.require_auth();
        Self::touch(&env);
        Self::check_request(&env, &route, &taker_token, &maker_token, min_out);

        let me = env.current_contract_address();
        let paid_in = token::Client::new(&env, &taker_token);
        let paid_out = token::Client::new(&env, &maker_token);
        let (declared, to_pull) = Self::plan(&env, &route, &taker, &taker_token, &maker_token);

        let opening = Opening {
            held_in: paid_in.balance(&me),
            held_out: paid_out.balance(&me),
            taker_in: paid_in.balance(&taker),
            taker_out: paid_out.balance(&taker),
        };

        Self::pull(&env, &paid_in, &me, &taker, to_pull, opening.held_in);
        for leg in route.iter() {
            Self::execute(&env, &me, &taker_token, &maker_token, &leg);
        }
        Self::forward(&paid_out, &me, &taker, opening.held_out);
        Self::forward(&paid_in, &me, &taker, opening.held_in);

        let result = RouteResult {
            taker_spent: opening.taker_in - paid_in.balance(&taker),
            amount_out: paid_out.balance(&taker) - opening.taker_out,
        };
        Self::check_result(&env, &result, min_out, declared);

        env.events().publish(
            (symbol_short!("route"), taker, taker_token, maker_token),
            (result.taker_spent, result.amount_out),
        );
        result
    }

    fn check_request(
        env: &Env,
        route: &Vec<Leg>,
        taker_token: &Address,
        maker_token: &Address,
        min_out: i128,
    ) {
        if Self::is_paused(env.clone()) {
            panic_with_error!(env, Error::Paused);
        }
        if route.is_empty() {
            panic_with_error!(env, Error::EmptyRoute);
        }
        if min_out <= 0 || taker_token == maker_token {
            panic_with_error!(env, Error::InvalidAmount);
        }
    }

    fn plan(
        env: &Env,
        route: &Vec<Leg>,
        taker: &Address,
        taker_token: &Address,
        maker_token: &Address,
    ) -> (i128, i128) {
        let mut declared = 0i128;
        let mut to_pull = 0i128;
        for leg in route.iter() {
            let (amount, routed) = Self::validate_leg(env, &leg, taker, taker_token, maker_token);
            declared += amount;
            if routed {
                to_pull += amount;
            }
        }
        (declared, to_pull)
    }

    fn pull(
        env: &Env,
        client: &token::Client,
        me: &Address,
        taker: &Address,
        amount: i128,
        held: i128,
    ) {
        if amount <= 0 {
            return;
        }
        client.transfer_from(me, taker, me, &amount);
        if client.balance(me) - held < amount {
            panic_with_error!(env, Error::InputShortfall);
        }
    }

    fn forward(client: &token::Client, me: &Address, to: &Address, held: i128) {
        let gained = client.balance(me) - held;
        if gained > 0 {
            client.transfer(me, to, &gained);
        }
    }

    fn check_result(env: &Env, result: &RouteResult, min_out: i128, declared: i128) {
        if result.amount_out < min_out {
            panic_with_error!(env, Error::BelowMinOut);
        }
        if result.taker_spent > declared {
            panic_with_error!(env, Error::OverSpent);
        }
    }

    fn validate_leg(
        env: &Env,
        leg: &Leg,
        taker: &Address,
        taker_token: &Address,
        maker_token: &Address,
    ) -> (i128, bool) {
        let (amount, routed) = match leg {
            Leg::Rfq(l) => Self::validate_signed_leg(env, l, taker, taker_token, maker_token),
            Leg::Dex(l) => (Self::validate_source_leg(env, l, SourceKind::Dex), true),
            Leg::Facility(l) => (
                Self::validate_source_leg(env, l, SourceKind::Facility),
                true,
            ),
        };
        if amount <= 0 {
            panic_with_error!(env, Error::InvalidAmount);
        }
        (amount, routed)
    }

    fn validate_signed_leg(
        env: &Env,
        leg: &RfqLeg,
        taker: &Address,
        taker_token: &Address,
        maker_token: &Address,
    ) -> (i128, bool) {
        let (leg_in, leg_out, leg_taker) = Self::leg_terms(&leg.order);
        if leg_in != taker_token || leg_out != maker_token {
            panic_with_error!(env, Error::TokenMismatch);
        }
        let routed = Self::is_routed(env, leg_taker, taker);
        let expected = if routed { 0 } else { 1 };
        if leg.taker_signature.len() != expected {
            panic_with_error!(env, Error::LegSignatureMismatch);
        }
        (leg.taker_amount, routed)
    }

    fn leg_terms(order: &SignedOrder) -> (&Address, &Address, &Option<Address>) {
        match order {
            SignedOrder::Rfq(o) => (&o.taker_token, &o.maker_token, &o.taker),
            SignedOrder::Fixed(o) => (&o.taker_token, &o.maker_token, &o.taker),
        }
    }

    fn is_routed(env: &Env, leg_taker: &Option<Address>, taker: &Address) -> bool {
        match leg_taker {
            None => true,
            Some(t) if t == taker => false,
            _ => panic_with_error!(env, Error::LegTakerMismatch),
        }
    }

    fn validate_source_leg(env: &Env, leg: &AggregatorLeg, kind: SourceKind) -> i128 {
        if Self::kind_of(env, &leg.aggregator) != Some(kind) {
            panic_with_error!(env, Error::SourceNotRegistered);
        }
        if leg.min_maker_amount <= 0 {
            panic_with_error!(env, Error::InvalidAmount);
        }
        leg.taker_amount
    }

    fn execute(env: &Env, me: &Address, taker_token: &Address, maker_token: &Address, leg: &Leg) {
        match leg {
            Leg::Rfq(l) => Self::submit(env, l, me, taker_token),
            Leg::Dex(l) | Leg::Facility(l) => Self::draw(env, l, me, taker_token, maker_token),
        }
    }

    /// An open bid makes the router the taker of record, so settlement pulls from
    /// it against an allowance expiring with this ledger. A bid naming a taker
    /// needs none: settlement pulls from that taker and pays them directly.
    fn submit(env: &Env, leg: &RfqLeg, me: &Address, taker_token: &Address) {
        let settlement = Self::settlement(env.clone());
        let taker_signature = leg.taker_signature.first();
        if taker_signature.is_none() {
            token::Client::new(env, taker_token).approve(
                me,
                &settlement,
                &leg.taker_amount,
                &env.ledger().sequence(),
            );
        }
        let client = SettlementClient::new(env, &settlement);
        match &leg.order {
            SignedOrder::Rfq(o) => client.fill_rfq_order(
                o,
                &leg.maker_signature,
                &taker_signature,
                me,
                &leg.taker_amount,
            ),
            SignedOrder::Fixed(o) => client.fill_fixed_order(
                o,
                &leg.maker_signature,
                &taker_signature,
                me,
                &leg.taker_amount,
            ),
        };
    }

    /// Holds the source to its quoted payout by measuring this contract's balance
    /// delta, not by reading the aggregator's return value.
    fn draw(
        env: &Env,
        leg: &AggregatorLeg,
        me: &Address,
        taker_token: &Address,
        maker_token: &Address,
    ) {
        let out = token::Client::new(env, maker_token);
        let before = out.balance(me);
        token::Client::new(env, taker_token).transfer(me, &leg.aggregator, &leg.taker_amount);
        AggregatorClient::new(env, &leg.aggregator).fill(
            me,
            taker_token,
            maker_token,
            &leg.taker_amount,
            &leg.min_maker_amount,
            &leg.data,
        );
        if out.balance(me) - before < leg.min_maker_amount {
            panic_with_error!(env, Error::SourceUnderDelivered);
        }
    }

    fn touch(env: &Env) {
        env.storage().instance().extend_ttl(THRESHOLD, EXTEND);
    }

    fn index_of(sources: &Vec<SourceEntry>, addr: &Address) -> Option<u32> {
        for (i, entry) in sources.iter().enumerate() {
            if &entry.address == addr {
                return Some(i as u32);
            }
        }
        None
    }

    fn kind_of(env: &Env, addr: &Address) -> Option<SourceKind> {
        Self::sources(env.clone())
            .iter()
            .find(|e| &e.address == addr)
            .map(|e| e.kind)
    }

    fn try_quote(
        env: &Env,
        source: &Address,
        taker_token: &Address,
        maker_token: &Address,
        taker_amount: i128,
    ) -> Option<i128> {
        if taker_amount <= 0 {
            return None;
        }
        match AggregatorClient::new(env, source).try_quote(taker_token, maker_token, &taker_amount)
        {
            Ok(Ok(q)) if q > 0 => Some(q),
            _ => None,
        }
    }
}
