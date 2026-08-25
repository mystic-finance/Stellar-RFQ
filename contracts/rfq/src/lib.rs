#![no_std]
//! Octarine settlement for Stellar — duration-priced RFQ, fixed orders and Dutch
//! listings over SEP-41 tokens. See README.md for the model and the flows.

mod errors;
mod hash;
mod price;
mod storage;
mod types;

#[cfg(test)]
mod invariant;
#[cfg(test)]
mod mock_token;
#[cfg(test)]
mod test;

use soroban_sdk::{
    contract, contractimpl, panic_with_error, symbol_short, token, xdr::ToXdr, Address, Bytes,
    BytesN, Env,
};

pub use errors::Error;
pub use types::{
    Config, DutchOrder, FillResult, FixedOrder, Listing, OracleCfg, OrderType, PriceData,
    PushedPrice, Quote, Reference, Request, RfqOrder, Schedule, ScheduleMode, Signature,
};

use price::{mul_div, mul_div_ceil, BPS, DENOM, ONE};

const MAX_FEE_BPS_LIMIT: u32 = 2_000;

#[contract]
pub struct RfqContract;

#[contractimpl]
impl RfqContract {
    // ---------------------------------------------------------------- setup

    pub fn initialize(env: Env, admin: Address, reference: Address) {
        if storage::has_admin(&env) {
            panic_with_error!(&env, Error::AlreadyInitialized);
        }
        storage::set_admin(&env, &admin);
        storage::set_reference(
            &env,
            &Reference {
                asset: reference,
                epoch: 1,
            },
        );
        storage::set_config(
            &env,
            &Config {
                min_fee_bps: 0,
                max_fee_bps: 1_000,
                fallback_max_age: 0,
                max_deviation_bps: 2_000,
                max_shift_seconds: 2 * 86_400,
                decay_seconds: 0,
            },
        );
        storage::extend_instance(&env);
    }

    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        storage::admin(&env).require_auth();
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    pub fn get_admin(env: Env) -> Address {
        storage::admin(&env)
    }

    pub fn get_config(env: Env) -> Config {
        storage::config(&env)
    }

    pub fn get_reference(env: Env) -> Reference {
        storage::reference(&env)
    }

    pub fn is_paused(env: Env) -> bool {
        storage::paused(&env)
    }

    // ----------------------------------------------------------- governance

    pub fn set_config(env: Env, cfg: Config) {
        storage::admin(&env).require_auth();
        if cfg.min_fee_bps > cfg.max_fee_bps
            || cfg.max_fee_bps > MAX_FEE_BPS_LIMIT
            || cfg.max_deviation_bps > BPS as u32
        {
            panic_with_error!(&env, Error::InvalidConfig);
        }
        storage::set_config(&env, &cfg);
        storage::extend_instance(&env);
    }

    /// Repoint the reference asset. Bumps the epoch, invalidating pushed prices.
    pub fn set_reference(env: Env, asset: Address) {
        storage::admin(&env).require_auth();
        let epoch = storage::reference(&env).epoch + 1;
        storage::set_reference(&env, &Reference { asset, epoch });
        storage::extend_instance(&env);
    }

    pub fn set_keeper(env: Env, keeper: Address, allowed: bool) {
        storage::admin(&env).require_auth();
        storage::set_keeper(&env, &keeper, allowed);
    }

    pub fn set_paused(env: Env, paused: bool) {
        storage::admin(&env).require_auth();
        storage::set_paused(&env, paused);
    }

    /// Register (`Some`) or clear (`None`) the feed for a `(base, quote)` pair.
    pub fn set_oracle(env: Env, base: Address, quote: Address, cfg: Option<OracleCfg>) {
        storage::admin(&env).require_auth();
        if let Some(c) = &cfg {
            if c.max_age == 0 {
                panic_with_error!(&env, Error::InvalidConfig);
            }
        }
        storage::set_oracle(&env, &base, &quote, &cfg);
    }

    pub fn set_schedule(env: Env, caller: Address, asset: Address, schedule: Schedule) {
        let is_admin = Self::keeper_auth(&env, &caller);
        price::validate_schedule(&env, &schedule);
        if !is_admin {
            let max = storage::config(&env).max_shift_seconds;
            if let Some(prev) = storage::schedule(&env, &asset) {
                let horizon = price::seconds_for(&env, &prev)
                    .abs_diff(price::seconds_for(&env, &schedule));
                let amplitude = Self::amplitude(&prev).abs_diff(Self::amplitude(&schedule));
                if max > 0 && (horizon > max || amplitude > max) {
                    panic_with_error!(&env, Error::ScheduleShiftTooLarge);
                }
            }
        }
        storage::set_schedule(&env, &asset, &schedule);
        env.events()
            .publish((symbol_short!("sched"), asset), schedule);
    }

    /// Backstop price of `asset` in the reference asset, 1e18-scaled.
    pub fn push_price(env: Env, caller: Address, asset: Address, new_price: i128) {
        Self::keeper_auth(&env, &caller);
        if new_price <= 0 {
            panic_with_error!(&env, Error::InvalidAmount);
        }
        let epoch = storage::reference(&env).epoch;
        let max_dev = storage::config(&env).max_deviation_bps;
        if let Some(prev) = storage::fallback(&env, &asset) {
            let diff = (new_price - prev.price).abs();
            if prev.epoch == epoch
                && max_dev > 0
                && mul_div(&env, diff, BPS, prev.price) > max_dev as i128
            {
                panic_with_error!(&env, Error::PriceDeviation);
            }
        }
        storage::set_fallback(
            &env,
            &asset,
            &PushedPrice {
                price: new_price,
                updated_at: env.ledger().timestamp(),
                epoch,
            },
        );
        env.events()
            .publish((symbol_short!("price"), asset), new_price);
    }

    // ---------------------------------------------------------------- views

    pub fn seconds_to_redemption(env: Env, asset: Address) -> u32 {
        price::seconds_to_redemption(&env, &asset)
    }

    pub fn get_schedule(env: Env, asset: Address) -> Option<Schedule> {
        storage::schedule(&env, &asset)
    }

    pub fn price_of(env: Env, base: Address, quote: Address) -> i128 {
        price::price_of(&env, &base, &quote)
    }

    pub fn hash_rfq_order(env: Env, order: RfqOrder) -> BytesN<32> {
        hash::rfq_order(&env, &order)
    }

    pub fn hash_fixed_order(env: Env, order: FixedOrder) -> BytesN<32> {
        hash::fixed_order(&env, &order)
    }

    pub fn filled_amount(env: Env, order_hash: BytesN<32>) -> i128 {
        storage::filled(&env, &order_hash)
    }

    pub fn hash_request(env: Env, request: Request) -> BytesN<32> {
        hash::request(&env, &request)
    }

    pub fn request_filled_amount(env: Env, request_hash: BytesN<32>) -> i128 {
        storage::request_filled(&env, &request_hash)
    }

    pub fn is_salt_cancelled(env: Env, signer: Address, salt: u64) -> bool {
        storage::is_salt_cancelled(&env, &signer, salt)
    }

    pub fn is_order_signer(env: Env, maker: Address, signer: BytesN<32>) -> bool {
        storage::is_signer(&env, &maker, &signer)
    }

    pub fn get_listing(env: Env, id: u64) -> Option<Listing> {
        storage::listing(&env, id)
    }

    // ---------------------------------------------- signers & cancellation

    pub fn register_order_signer(env: Env, maker: Address, signer: BytesN<32>, allowed: bool) {
        maker.require_auth();
        storage::set_signer(&env, &maker, &signer, allowed);
    }

    /// Void every unfilled order `signer` put under this salt — both the taker's
    /// request and any maker bid adopting it. Callable by the signer or by a key
    /// they registered.
    pub fn cancel_salt(env: Env, caller: Address, signer: Address, salt: u64) {
        caller.require_auth();
        // A delegated signer is registered by its ed25519 key, and a G... address
        // is that key, so a hot key can retract the book it signed.
        let delegated = Self::account_pubkey(&env, &caller)
            .map(|pk| storage::is_signer(&env, &signer, &pk))
            .unwrap_or(false);
        if caller != signer && !delegated {
            panic_with_error!(&env, Error::NotAuthorized);
        }
        storage::set_salt_cancelled(&env, &signer, salt);
        env.events()
            .publish((symbol_short!("salt_cxl"), signer), salt);
    }

    // ------------------------------------------------------------ RFQ fills

    pub fn quote_rfq_order(env: Env, order: RfqOrder, taker_amount_in: i128) -> Quote {
        Self::quote(&env, &order, taker_amount_in)
    }

    pub fn fill_rfq_order(
        env: Env,
        order: RfqOrder,
        maker_signature: Signature,
        taker_signature: Option<Signature>,
        sender: Address,
        taker_amount_in: i128,
    ) -> FillResult {
        sender.require_auth();
        let order_hash = hash::rfq_order(&env, &order);
        let taker = Self::validate_common(
            &env,
            &order.request(),
            &order.maker,
            &taker_signature,
            &sender,
            taker_amount_in,
        );

        Self::take_fill(&env, &order_hash, order.taker_amount, taker_amount_in);
        Self::verify(&env, &order.maker, &order_hash, &maker_signature);

        let delivered = Self::deliver(
            &env,
            &order.taker_token,
            &taker,
            &order.maker,
            taker_amount_in,
        );
        let q = Self::quote(&env, &order, delivered);
        let cap = mul_div(
            &env,
            order.max_maker_amount,
            taker_amount_in,
            order.taker_amount,
        );
        if q.maker_amount > cap {
            panic_with_error!(&env, Error::MakerAmountTooHigh);
        }

        let received = Self::pay(
            &env,
            &order.maker_token,
            &order.maker,
            &taker,
            &order.fee_recipient,
            q.maker_amount - q.fee,
            q.fee,
        );
        Self::check_floor(
            &env,
            received,
            order.min_received_amount,
            taker_amount_in,
            order.taker_amount,
        );

        env.events().publish(
            (symbol_short!("rfq_fill"), order_hash, order.maker, taker),
            (
                taker_amount_in,
                q.maker_amount,
                q.fee,
                q.horizon_seconds,
                order.maker_bps_per_day,
            ),
        );
        FillResult {
            taker_filled: taker_amount_in,
            maker_filled: q.maker_amount,
            fee: q.fee,
        }
    }

    // ---------------------------------------------------------- fixed fills

    pub fn fill_fixed_order(
        env: Env,
        order: FixedOrder,
        maker_signature: Signature,
        taker_signature: Option<Signature>,
        sender: Address,
        taker_amount_in: i128,
    ) -> FillResult {
        sender.require_auth();
        let order_hash = hash::fixed_order(&env, &order);
        let taker = Self::validate_common(
            &env,
            &order.request(),
            &order.maker,
            &taker_signature,
            &sender,
            taker_amount_in,
        );

        Self::take_fill(&env, &order_hash, order.taker_amount, taker_amount_in);
        Self::verify(&env, &order.maker, &order_hash, &maker_signature);

        let delivered = Self::deliver(
            &env,
            &order.taker_token,
            &taker,
            &order.maker,
            taker_amount_in,
        );
        let maker_amount = mul_div(&env, order.maker_amount, delivered, order.taker_amount);
        let fee = mul_div(&env, maker_amount, order.fee_bps as i128, BPS);

        let received = Self::pay(
            &env,
            &order.maker_token,
            &order.maker,
            &taker,
            &order.fee_recipient,
            maker_amount - fee,
            fee,
        );
        Self::check_floor(
            &env,
            received,
            order.min_received_amount,
            taker_amount_in,
            order.taker_amount,
        );

        env.events().publish(
            (symbol_short!("fix_fill"), order_hash, order.maker, taker),
            (taker_amount_in, maker_amount, fee),
        );
        FillResult {
            taker_filled: taker_amount_in,
            maker_filled: maker_amount,
            fee,
        }
    }

    // -------------------------------------------------------- Dutch listings

    pub fn create_dutch_order(env: Env, seller: Address, order: DutchOrder) -> u64 {
        seller.require_auth();
        Self::not_paused(&env);
        if order.maker_token == order.taker_token {
            panic_with_error!(&env, Error::SameToken);
        }
        if order.taker_amount <= 0
            || order.min_maker_amount <= 0
            || order.start_maker_amount < order.min_maker_amount
        {
            panic_with_error!(&env, Error::InvalidAmount);
        }
        if order.expiry != 0 && order.expiry <= env.ledger().timestamp() {
            panic_with_error!(&env, Error::OrderNotFillable);
        }
        Self::check_fee(&env, order.fee_bps);

        let escrowed = Self::deliver(
            &env,
            &order.taker_token,
            &seller,
            &env.current_contract_address(),
            order.taker_amount,
        );
        let mut terms = order;
        if escrowed != terms.taker_amount {
            terms.start_maker_amount =
                mul_div(&env, terms.start_maker_amount, escrowed, terms.taker_amount);
            terms.min_maker_amount =
                mul_div(&env, terms.min_maker_amount, escrowed, terms.taker_amount);
            terms.taker_amount = escrowed;
            if terms.min_maker_amount <= 0 {
                panic_with_error!(&env, Error::InvalidAmount);
            }
        }

        let id = storage::next_id(&env);
        storage::set_listing(
            &env,
            id,
            &Listing {
                order: terms,
                seller: seller.clone(),
                created_at: env.ledger().timestamp(),
                decay_seconds: storage::config(&env).decay_seconds,
                active: true,
            },
        );
        storage::extend_instance(&env);
        env.events()
            .publish((symbol_short!("dutch_new"), id), (seller, escrowed));
        id
    }

    pub fn current_ask(env: Env, id: u64) -> i128 {
        Self::ask(&env, &Self::active_listing(&env, id))
    }

    pub fn fill_dutch_order(
        env: Env,
        id: u64,
        buyer: Address,
        max_maker_amount: i128,
    ) -> FillResult {
        buyer.require_auth();
        Self::not_paused(&env);
        let mut listing = Self::active_listing(&env, id);
        if listing.order.expiry != 0 && env.ledger().timestamp() > listing.order.expiry {
            panic_with_error!(&env, Error::OrderNotFillable);
        }
        Self::check_fee(&env, listing.order.fee_bps);

        let maker_amount = Self::ask(&env, &listing);
        if maker_amount <= 0 {
            panic_with_error!(&env, Error::InvalidAmount);
        }
        if maker_amount > max_maker_amount {
            panic_with_error!(&env, Error::AskAboveMax);
        }
        let fee = mul_div(&env, maker_amount, listing.order.fee_bps as i128, BPS);

        listing.active = false;
        storage::set_listing(&env, id, &listing);

        token::Client::new(&env, &listing.order.taker_token).transfer(
            &env.current_contract_address(),
            &buyer,
            &listing.order.taker_amount,
        );
        let received = Self::pay(
            &env,
            &listing.order.maker_token,
            &buyer,
            &listing.seller,
            &listing.order.fee_recipient,
            maker_amount - fee,
            fee,
        );
        if received < maker_amount - fee {
            panic_with_error!(&env, Error::BelowMinReceived);
        }

        env.events()
            .publish((symbol_short!("dutch_fil"), id, buyer), (maker_amount, fee));
        FillResult {
            taker_filled: listing.order.taker_amount,
            maker_filled: maker_amount,
            fee,
        }
    }

    pub fn cancel_dutch_order(env: Env, id: u64) {
        let mut listing = Self::active_listing(&env, id);
        listing.seller.require_auth();
        listing.active = false;
        storage::set_listing(&env, id, &listing);
        token::Client::new(&env, &listing.order.taker_token).transfer(
            &env.current_contract_address(),
            &listing.seller,
            &listing.order.taker_amount,
        );
        env.events().publish((symbol_short!("dutch_cxl"), id), ());
    }

    // ------------------------------------------------------------ internals

    fn quote(env: &Env, order: &RfqOrder, taker_amount_in: i128) -> Quote {
        let (horizon_seconds, cap) = price::horizon(env, &order.taker_token, &order.maker_token);
        if order.maker_bps_per_day > cap || order.maker_bps_per_day > order.taker_max_bps_per_day {
            panic_with_error!(env, Error::BpsPerDayTooHigh);
        }
        let discount = order.maker_bps_per_day as i128 * horizon_seconds as i128;
        if discount >= DENOM {
            panic_with_error!(env, Error::DiscountTooLarge);
        }
        let rate = price::price_of(env, &order.taker_token, &order.maker_token);
        let gross = mul_div(env, taker_amount_in, rate, ONE);
        let maker_amount = mul_div(env, gross, DENOM - discount, DENOM);
        Quote {
            maker_amount,
            fee: mul_div(env, maker_amount, order.fee_bps as i128, BPS),
            horizon_seconds,
        }
    }

    fn ask(env: &Env, listing: &Listing) -> i128 {
        let start = listing.order.start_maker_amount;
        let floor = listing.order.min_maker_amount;
        let elapsed = env.ledger().timestamp() - listing.created_at;
        if listing.decay_seconds == 0 || elapsed >= listing.decay_seconds as u64 {
            return floor;
        }
        start
            - mul_div(
                env,
                start - floor,
                elapsed as i128,
                listing.decay_seconds as i128,
            )
    }

    /// Every check both order types share, plus the taker's consent, in one
    /// place. Returns the resolved taker: the request's, or the sender on an
    /// open request.
    fn validate_common(
        env: &Env,
        r: &Request,
        maker: &Address,
        taker_signature: &Option<Signature>,
        sender: &Address,
        taker_amount_in: i128,
    ) -> Address {
        Self::not_paused(env);
        if r.maker_token == r.taker_token {
            panic_with_error!(env, Error::SameToken);
        }
        if taker_amount_in <= 0 || r.taker_amount <= 0 {
            panic_with_error!(env, Error::InvalidAmount);
        }
        if env.ledger().timestamp() >= r.expiry {
            panic_with_error!(env, Error::OrderNotFillable);
        }
        if let Some(only) = &r.sender {
            if only != sender {
                panic_with_error!(env, Error::WrongSender);
            }
        }
        if r.min_received_amount <= 0 {
            panic_with_error!(env, Error::InvalidAmount);
        }
        Self::check_fee(env, r.fee_bps);
        if storage::is_salt_cancelled(env, maker, r.salt) {
            panic_with_error!(env, Error::SaltIsCancelled);
        }

        let taker = match &r.taker {
            None => return sender.clone(),
            Some(t) => t.clone(),
        };
        if storage::is_salt_cancelled(env, &taker, r.salt) {
            panic_with_error!(env, Error::SaltIsCancelled);
        }

        // A named taker consents off-chain, so anyone may submit the fill. When
        // the taker submits it themselves their own auth already said so.
        let request_hash = hash::request(env, r);
        if &taker != sender {
            match taker_signature {
                Some(sig) => Self::verify(env, &taker, &request_hash, sig),
                None => panic_with_error!(env, Error::SignerNotAuthorized),
            }
        }

        let filled = storage::request_filled(env, &request_hash);
        if filled + taker_amount_in > r.taker_amount {
            panic_with_error!(env, Error::RequestOverfilled);
        }
        storage::set_request_filled(env, &request_hash, filled + taker_amount_in);
        taker
    }

    /// Books the fill against the maker's order before any funds move.
    fn take_fill(env: &Env, order_hash: &BytesN<32>, taker_amount: i128, taker_amount_in: i128) {
        let filled = storage::filled(env, order_hash);
        if filled + taker_amount_in > taker_amount {
            panic_with_error!(env, Error::OrderNotFillable);
        }
        storage::set_filled(env, order_hash, filled + taker_amount_in);
    }

    /// Moves the taker leg and reports what the receiver was actually credited,
    /// clamped to `amount`.
    fn deliver(env: &Env, tok: &Address, from: &Address, to: &Address, amount: i128) -> i128 {
        let client = token::Client::new(env, tok);
        let before = client.balance(to);
        client.transfer_from(&env.current_contract_address(), from, to, &amount);
        let received = client.balance(to) - before;
        if received <= 0 {
            panic_with_error!(env, Error::InvalidAmount);
        }
        if received > amount {
            amount
        } else {
            received
        }
    }

    /// Counterparty and fee recipient are both paid out of `payer`'s pocket.
    fn pay(
        env: &Env,
        tok: &Address,
        payer: &Address,
        to: &Address,
        fee_to: &Address,
        net: i128,
        fee: i128,
    ) -> i128 {
        let client = token::Client::new(env, tok);
        let me = env.current_contract_address();
        let before = client.balance(to);
        if net > 0 {
            client.transfer_from(&me, payer, to, &net);
        }
        if fee > 0 {
            client.transfer_from(&me, payer, fee_to, &fee);
        }
        client.balance(to) - before
    }

    /// The taker's signed floor, pro-rated to this slice and rounded up.
    fn check_floor(
        env: &Env,
        received: i128,
        min_received: i128,
        taker_amount_in: i128,
        taker_amount: i128,
    ) {
        if received < mul_div_ceil(env, min_received, taker_amount_in, taker_amount) {
            panic_with_error!(env, Error::BelowMinReceived);
        }
    }

    fn verify(env: &Env, maker: &Address, order_hash: &BytesN<32>, signature: &Signature) {
        let digest = hash::sep53(env, order_hash);
        env.crypto().ed25519_verify(
            &signature.signer,
            &Bytes::from_array(env, &digest.to_array()),
            &signature.signature,
        );
        let own = Self::account_pubkey(env, maker)
            .map(|pk| pk == signature.signer)
            .unwrap_or(false);
        if !own && !storage::is_signer(env, maker, &signature.signer) {
            panic_with_error!(env, Error::SignerNotAuthorized);
        }
    }

    /// ed25519 key of a `G…` account address; `None` for contract addresses.
    fn account_pubkey(env: &Env, maker: &Address) -> Option<BytesN<32>> {
        let xdr = maker.clone().to_xdr(env);
        if xdr.len() != 44 {
            return None;
        }
        let mut pk = [0u8; 32];
        for (i, b) in pk.iter_mut().enumerate() {
            *b = xdr.get(12 + i as u32).unwrap();
        }
        Some(BytesN::from_array(env, &pk))
    }

    fn active_listing(env: &Env, id: u64) -> Listing {
        match storage::listing(env, id) {
            Some(l) if l.active => l,
            _ => panic_with_error!(env, Error::ListingNotActive),
        }
    }

    fn check_fee(env: &Env, fee_bps: u32) {
        let cfg = storage::config(env);
        if fee_bps < cfg.min_fee_bps || fee_bps > cfg.max_fee_bps {
            panic_with_error!(env, Error::FeeOutOfBounds);
        }
    }

    fn not_paused(env: &Env) {
        if storage::paused(env) {
            panic_with_error!(env, Error::Paused);
        }
    }

    /// Authorises admin or a registered keeper; returns whether it was the admin.
    fn keeper_auth(env: &Env, caller: &Address) -> bool {
        caller.require_auth();
        if caller == &storage::admin(env) {
            return true;
        }
        if !storage::is_keeper(env, caller) {
            panic_with_error!(env, Error::NotAuthorized);
        }
        false
    }

    fn amplitude(s: &Schedule) -> u32 {
        if s.mode == ScheduleMode::Rolling {
            s.rolling_seconds
        } else {
            s.cycle_seconds
        }
    }
}
