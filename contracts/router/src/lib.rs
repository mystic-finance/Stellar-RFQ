#![no_std]
//! Octarine RFQ router — settles the route the taker chose atomically across
//! signed LP bids, the facility aggregator and DEX aggregators, and holds the
//! result to the taker's minimum output. The auction, the ranking and the route
//! choice all happen off-chain; this contract executes the decision.
//!
//! It takes no fee of its own: every venue skims where it settles, so a routed
//! trade is charged exactly once. See README.md for the routing model.

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

#[contract]
pub struct RouterContract;

#[contractimpl]
impl RouterContract {
    // ---------------------------------------------------------------- setup

    pub fn initialize(env: Env, admin: Address, settlement: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&env, Error::AlreadyInitialized);
        }
        let store = env.storage().instance();
        store.set(&DataKey::Admin, &admin);
        store.set(&DataKey::Settlement, &settlement);
        store.set(&DataKey::Sources, &Vec::<SourceEntry>::new(&env));
    }

    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        Self::admin(env.clone()).require_auth();
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    // ----------------------------------------------------------- governance

    /// Whitelist (or drop) an aggregator, as either a DEX or a facility source.
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
        env.events()
            .publish((symbol_short!("source"), source), (kind, allowed));
    }

    pub fn set_settlement(env: Env, settlement: Address) {
        Self::admin(env.clone()).require_auth();
        env.storage()
            .instance()
            .set(&DataKey::Settlement, &settlement);
    }

    pub fn set_paused(env: Env, paused: bool) {
        Self::admin(env.clone()).require_auth();
        env.storage().instance().set(&DataKey::Paused, &paused);
    }

    // ---------------------------------------------------------------- views

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

    /// Every registered source's price for this trade size — read-only, for the
    /// backend to rank on-chain bids alongside the signed book it collected. A
    /// source that traps or quotes nothing is left out rather than breaking the
    /// sweep; one needing extra parameters is quoted by the backend directly.
    pub fn quote(
        env: Env,
        taker_token: Address,
        maker_token: Address,
        taker_amount: i128,
    ) -> Vec<SourceQuote> {
        let mut out = Vec::new(&env);
        for entry in Self::sources(env.clone()).iter() {
            if let Some(maker_amount) =
                Self::try_quote(&env, &entry.address, &taker_token, &maker_token, taker_amount)
            {
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
            if best.as_ref().is_none_or(|b| q.maker_amount > b.maker_amount) {
                best = Some(q);
            }
        }
        best
    }

    // --------------------------------------------------------------- filling

    /// Settle the route the taker chose, in one transaction.
    ///
    /// The router pulls the whole input from the taker once, then **transfers to
    /// each venue and calls it** — the way mainstream aggregators work — so no
    /// venue ever holds an allowance on the taker and the taker approves only
    /// this contract. Output lands here, the protocol fee is skimmed, and the
    /// taker is paid the net. Anything left unspent is refunded. The router never
    /// re-picks a leg: the route is the taker's decision, made against the bids
    /// the backend showed them.
    pub fn fill(
        env: Env,
        taker: Address,
        taker_token: Address,
        maker_token: Address,
        route: Vec<Leg>,
        min_out: i128,
    ) -> RouteResult {
        taker.require_auth();
        if Self::is_paused(env.clone()) {
            panic_with_error!(&env, Error::Paused);
        }
        if route.is_empty() {
            panic_with_error!(&env, Error::EmptyRoute);
        }
        if min_out <= 0 || taker_token == maker_token {
            panic_with_error!(&env, Error::InvalidAmount);
        }

        let me = env.current_contract_address();
        let taker_client = token::Client::new(&env, &taker_token);
        let maker_client = token::Client::new(&env, &maker_token);

        // Validate the whole route before a single token moves, so a malformed
        // leg costs nothing and fails for the reason it is actually wrong.
        // `to_pull` counts only the legs the router pays for.
        let mut declared = 0i128;
        let mut to_pull = 0i128;
        for leg in route.iter() {
            let (amount, routed) = Self::validate_leg(&env, &leg, &taker, &taker_token, &maker_token);
            declared += amount;
            if routed {
                to_pull += amount;
            }
        }

        // Balances the router already held stay untouched; everything below is a
        // delta against them, so stray dust can never be swept into a route.
        let held_in = taker_client.balance(&me);
        let held_out = maker_client.balance(&me);
        let spent_before = taker_client.balance(&taker);
        let out_before = maker_client.balance(&taker);

        if to_pull > 0 {
            taker_client.transfer_from(&me, &taker, &me, &to_pull);
            if taker_client.balance(&me) - held_in < to_pull {
                panic_with_error!(&env, Error::InputShortfall);
            }
        }

        for leg in route.iter() {
            Self::execute(&env, &me, &taker_token, &maker_token, &leg);
        }

        // Aggregator legs paid the router; signed legs paid the taker directly.
        // Forward the former, refund anything the legs did not consume, then
        // measure the taker's own position — which covers both paths.
        let collected = maker_client.balance(&me) - held_out;
        if collected > 0 {
            maker_client.transfer(&me, &taker, &collected);
        }
        let unspent = taker_client.balance(&me) - held_in;
        if unspent > 0 {
            taker_client.transfer(&me, &taker, &unspent);
        }

        let amount_out = maker_client.balance(&taker) - out_before;
        if amount_out < min_out {
            panic_with_error!(&env, Error::BelowMinOut);
        }

        let taker_spent = spent_before - taker_client.balance(&taker);
        // No venue may reach past its leg into an allowance the taker granted it
        // elsewhere and spend more than this route declared.
        if taker_spent > declared {
            panic_with_error!(&env, Error::OverSpent);
        }

        env.events().publish(
            (symbol_short!("route"), taker, taker_token, maker_token),
            (taker_spent, amount_out),
        );
        RouteResult {
            taker_spent,
            amount_out,
        }
    }

    // ------------------------------------------------------------ internals

    /// Checks one leg. Returns the taker amount it may spend, and whether the
    /// router pays for it — signed legs settle straight between counterparties.
    fn validate_leg(
        env: &Env,
        leg: &Leg,
        taker: &Address,
        taker_token: &Address,
        maker_token: &Address,
    ) -> (i128, bool) {
        let (amount, routed) = match leg {
            Leg::Rfq(l) => {
                let (leg_taker_token, leg_maker_token, leg_taker) = match &l.order {
                    SignedOrder::Rfq(o) => (&o.taker_token, &o.maker_token, &o.taker),
                    SignedOrder::Fixed(o) => (&o.taker_token, &o.maker_token, &o.taker),
                };
                if leg_taker_token != taker_token || leg_maker_token != maker_token {
                    panic_with_error!(env, Error::TokenMismatch);
                }
                // A bid quoted to a named taker settles straight between the
                // counterparties; one quoted to nobody is open liquidity the
                // router takes on its own behalf. A bid quoted to a *third party*
                // would move a stranger's funds, so it is refused.
                let routed = match leg_taker {
                    None => true,
                    Some(t) if t == taker => false,
                    _ => panic_with_error!(env, Error::LegTakerMismatch),
                };
                // The signature must match the mode, so a leg cannot claim one
                // shape and carry data for the other.
                let expected = if routed { 0 } else { 1 };
                if l.taker_signature.len() != expected {
                    panic_with_error!(env, Error::LegSignatureMismatch);
                }
                (l.taker_amount, routed)
            }
            Leg::Dex(l) | Leg::Facility(l) => {
                let kind = match leg {
                    Leg::Dex(_) => SourceKind::Dex,
                    _ => SourceKind::Facility,
                };
                if Self::kind_of(env, &l.aggregator) != Some(kind) {
                    panic_with_error!(env, Error::SourceNotRegistered);
                }
                if l.min_maker_amount <= 0 {
                    panic_with_error!(env, Error::InvalidAmount);
                }
                (l.taker_amount, true)
            }
        };
        if amount <= 0 {
            panic_with_error!(env, Error::InvalidAmount);
        }
        (amount, routed)
    }

    /// Runs one leg. The router already holds the input; every venue is paid up
    /// front and pays its output back to the router.
    fn execute(
        env: &Env,
        me: &Address,
        taker_token: &Address,
        maker_token: &Address,
        leg: &Leg,
    ) {
        match leg {
            Leg::Rfq(l) => {
                let settlement = Self::settlement(env.clone());
                let taker_signature = l.taker_signature.first();
                if taker_signature.is_none() {
                    // Open bid: the router is the taker of record, so settlement
                    // pulls from it. Allowance for exactly this leg, expiring with
                    // the current ledger.
                    token::Client::new(env, taker_token).approve(
                        me,
                        &settlement,
                        &l.taker_amount,
                        &env.ledger().sequence(),
                    );
                }
                // Otherwise nothing to move: settlement pulls from the taker's
                // own allowance and pays them directly. Either way the router
                // only submits, carrying the parties' off-chain signatures.
                let client = SettlementClient::new(env, &settlement);
                match &l.order {
                    SignedOrder::Rfq(o) => client.fill_rfq_order(
                        o,
                        &l.maker_signature,
                        &taker_signature,
                        me,
                        &l.taker_amount,
                    ),
                    SignedOrder::Fixed(o) => client.fill_fixed_order(
                        o,
                        &l.maker_signature,
                        &taker_signature,
                        me,
                        &l.taker_amount,
                    ),
                };
            }
            Leg::Dex(l) | Leg::Facility(l) => {
                Self::draw(env, l, me, taker_token, maker_token)
            }
        }
    }

    /// Transfers the leg's input to the aggregator, then calls it — and holds it
    /// to the payout the taker was shown when they picked this route.
    fn draw(
        env: &Env,
        leg: &AggregatorLeg,
        me: &Address,
        taker_token: &Address,
        maker_token: &Address,
    ) {
        token::Client::new(env, taker_token).transfer(me, &leg.aggregator, &leg.taker_amount);
        let delivered = AggregatorClient::new(env, &leg.aggregator).fill(
            me,
            taker_token,
            maker_token,
            &leg.taker_amount,
            &leg.min_maker_amount,
            &leg.data,
        );
        if delivered < leg.min_maker_amount {
            panic_with_error!(env, Error::SourceUnderDelivered);
        }
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
        match AggregatorClient::new(env, source).try_quote(taker_token, maker_token, &taker_amount) {
            Ok(Ok(q)) if q > 0 => Some(q),
            _ => None,
        }
    }

}
