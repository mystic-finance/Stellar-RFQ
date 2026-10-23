#![no_std]
//! Cross-chain RFQ fills, Stellar side, with the UX of a local fill: the taker
//! calls `fill` once and the bridge delivers everything after. One build per
//! bridge, its `transport` chosen at compile time: `lz_settle` (LayerZero
//! messages, payments through an OFT) and, with feature `cctp`, `cctp_settle`
//! (CCTP messages and burns). See docs/CROSSCHAIN_SETTLEMENT.md.
//!
//! - Taker here: `fill` prices the request at the maker's rate, escrows the RWA
//!   and sends FILL. The payment comes back with PAID and settles; a FAIL
//!   returns the RWA. CROSS_DELIVER then bridges the RWA to the maker.
//! - Maker here: a FILL arrives, the maker's signature is checked, the amount
//!   is pulled from the maker's standing approval and bridged back with PAID,
//!   or FAIL is sent.
//!
//! Config, peers, routes and redemption vaults change only through the timelock.
#![allow(clippy::too_many_arguments)]

#[cfg(feature = "cctp")]
#[path = "cctp.rs"]
pub mod transport;
#[cfg(not(feature = "cctp"))]
#[path = "lz.rs"]
pub mod transport;
pub mod wire;

#[cfg(all(test, not(feature = "cctp")))]
mod test;
#[cfg(all(test, feature = "cctp"))]
#[path = "test.rs"]
mod test_cctp;

use common::timelock;
pub use orders::{OrderType, Request, RfqOrder};
use soroban_sdk::auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation};
use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, panic_with_error,
    symbol_short, token, vec, Address, Bytes, BytesN, Env, IntoVal, Symbol, Val,
};
pub use wire::Bid;

use wire::{FAIL, PAID, PAYOUT};

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    UnknownSender = 1,
    NoPeer = 2,
    NoRoute = 3,
    NoVault = 4,
    Expired = 5,
    AlreadyFilled = 6,
    InvalidBid = 7,
    QuoteRejected = 8,
    NotFound = 9,
    NotAuthorized = 10,
    Underpaid = 11,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    /// The RFQ contract: request hashes, quotes, delegated signers, cancelled salts.
    pub rfq: Address,
    pub verifier: Address,
    /// Native XLM: `fill` takes the return fee in it, LayerZero its fees.
    pub xlm: Address,
    /// Out of what every payment's recipient gets, in wire units (6 decimals),
    /// for whoever `complete`s its delivery, else the recipient: flat, as the
    /// gas it covers is.
    pub tip: i128,
    pub bridge: transport::Bridge,
}

/// Our contract on another chain, by the bridge's chain id (a CCTP domain or a
/// LayerZero EID).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Peer {
    pub addr: BytesN<32>,
    /// XLM `fill` adds for the return leg, which the peer pays from its balance.
    pub return_fee: i128,
    /// LayerZero executor options for messages and OFT sends to it (CCTP: unused).
    pub options: Bytes,
}

/// Governed changes. `Vault` sets where an RWA redeems for the REDEEM modes.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(clippy::large_enum_variant)] // contract types cannot box
pub enum Change {
    Config(Config),
    Peer(u32, Peer),
    Route(u32, Address, Option<BytesN<32>>),
    Vault(Address, Option<Address>),
    Delay(u64),
}

/// A fill in flight, taker here. `amount` is the quote the payment must cover.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Order {
    pub taker: Address,
    pub chain: u32,
    pub rwa: Address,
    pub units: i128,
    pub stable: Address,
    pub amount: i128,
    pub fee: i128,
    pub tip: i128,
    pub fee_recipient: Address,
    pub delivery: u32,
    pub recipient: Address,
    pub maker: BytesN<32>,
    pub maker_is_contract: bool,
}

/// A REDEEM_PUSH claim held for a remote maker. RWA the issuer returns in
/// kind goes to the bid's `recipient`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Push {
    pub vault: Address,
    pub rwa: Address,
    pub recipient: Address,
    pub batch: u64,
    pub units: i128,
    pub chain: u32,
    pub stable: Address,
    pub maker: BytesN<32>,
    pub maker_is_contract: bool,
}

/// What the RFQ contract quotes: the maker's gross amount, the fee within it.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Quote {
    pub maker_amount: i128,
    pub fee: i128,
    pub horizon_seconds: u32,
}

#[contractclient(name = "RfqClient")]
pub trait Rfq {
    fn hash_request(env: Env, request: Request) -> BytesN<32>;
    fn quote_rfq_order(env: Env, order: RfqOrder, taker_amount_in: i128) -> Quote;
    fn is_order_signer(env: Env, maker: Address, signer: BytesN<32>) -> bool;
    fn is_salt_cancelled(env: Env, signer: Address, salt: u64) -> bool;
}

/// The redemption vault's fixed settings, as it reports them.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaultInit {
    pub rwa: Address,
    pub stable: Address,
    pub ct_wasm: BytesN<32>,
    pub account_wasm: BytesN<32>,
}

#[contractclient(name = "VaultClient")]
pub trait RedemptionVault {
    fn init(env: Env) -> VaultInit;
    fn request_redeem(env: Env, owner: Address, units: i128, controller: Address) -> u64;
    fn claim(env: Env, owner: Address, id: u64, shares: i128, receiver: Address) -> i128;
    fn ct_of(env: Env, id: u64) -> Address;
}

#[contractclient(name = "VerifierClient")]
pub trait Verifier {
    fn verify(env: Env, key: BytesN<32>, message: Bytes, signature: BytesN<64>);
}

#[contracttype]
enum Key {
    Admin,
    Config,
    Peer(u32),
    Route(u32, Address),
    Vault(Address),
    Order(BytesN<32>),
    Filled(BytesN<32>),
    Used(BytesN<32>),
    Push(BytesN<32>),
    Owed(Address, Address),
    Inbox(BytesN<32>),
}

const DELIVER: u32 = 0;
const REDEEM_CT: u32 = 1;
const REDEEM_PUSH: u32 = 2;
const CROSS_DELIVER: u32 = 3;
const THRESHOLD: u32 = 518_400;
const EXTEND: u32 = 535_680;

#[contract]
pub struct CrossChainSettle;

#[contractimpl]
impl CrossChainSettle {
    /// Deploy with `delay` 0 to configure, then queue the real delay.
    pub fn __constructor(env: Env, admin: Address, guardian: Address, delay: u64, config: Config) {
        timelock::init(&env, delay, &guardian);
        env.storage().instance().set(&Key::Admin, &admin);
        env.storage().instance().set(&Key::Config, &config);
        transport::init(&env, &config, admin);
    }

    pub fn config(env: Env) -> Config {
        env.storage().instance().get(&Key::Config).unwrap()
    }

    pub fn peer(env: Env, chain: u32) -> Option<Peer> {
        env.storage().instance().get(&Key::Peer(chain))
    }

    pub fn route(env: Env, chain: u32, stable: Address) -> Option<BytesN<32>> {
        env.storage().instance().get(&Key::Route(chain, stable))
    }

    pub fn vault(env: Env, rwa: Address) -> Option<Address> {
        env.storage().instance().get(&Key::Vault(rwa))
    }

    /// Admin: queues `change`, applied by `accept` once the delay has passed.
    /// Changes to different peers, routes and vaults queue independently.
    pub fn submit(env: Env, change: Change) -> u64 {
        Self::admin(&env).require_auth();
        Self::check_change(&env, &change);
        timelock::submit(&env, Self::kind_of(env.clone(), change.clone()), &change)
    }

    /// Anyone applies the change pending under `kind` once its delay has passed.
    pub fn accept(env: Env, kind: u32) {
        let change: Change = timelock::take(&env, kind);
        Self::check_change(&env, &change);
        let s = env.storage().instance();
        match change {
            Change::Config(c) => s.set(&Key::Config, &c),
            Change::Peer(chain, p) => s.set(&Key::Peer(chain), &p),
            Change::Route(eid, stable, remote) => {
                Self::set_or_remove(&env, &Key::Route(eid, stable), remote)
            }
            Change::Vault(rwa, vault) => Self::set_or_remove(&env, &Key::Vault(rwa), vault),
            Change::Delay(d) => timelock::init(&env, d, &timelock::guardian(&env)),
        }
    }

    pub fn pending(env: Env, kind: u32) -> Option<(Change, u64)> {
        timelock::pending(&env, kind)
    }

    pub fn revoke(env: Env, kind: u32, caller: Address) {
        timelock::revoke(&env, kind, &caller, &Self::admin(&env));
    }

    /// The timelock slot `change` queues under.
    pub fn kind_of(env: Env, change: Change) -> u32 {
        match change {
            Change::Config(_) => 0,
            Change::Delay(_) => 1,
            Change::Peer(eid, _) => timelock::kind_of(&env, (symbol_short!("peer"), eid)),
            Change::Route(eid, stable, _) => {
                timelock::kind_of(&env, (symbol_short!("route"), eid, stable))
            }
            Change::Vault(rwa, _) => timelock::kind_of(&env, (symbol_short!("vault"), rwa)),
        }
    }

    /// Moves XLM out, to rebalance return-leg funds between chains.
    pub fn sweep(env: Env, to: Address, amount: i128) {
        Self::admin(&env).require_auth();
        let cfg = Self::config(env.clone());
        token::Client::new(&env, &cfg.xlm).transfer(&env.current_contract_address(), &to, &amount);
    }

    pub fn order(env: Env, id: BytesN<32>) -> Option<Order> {
        env.storage().persistent().get(&Key::Order(id))
    }

    pub fn filled(env: Env, id: BytesN<32>) -> bool {
        env.storage().persistent().has(&Key::Filled(id))
    }

    pub fn push_of(env: Env, id: BytesN<32>) -> Option<Push> {
        env.storage().persistent().get(&Key::Push(id))
    }

    pub fn owed(env: Env, recipient: Address, token: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&Key::Owed(recipient, token))
            .unwrap_or(0)
    }

    // ---- taker here

    /// Accepts `bid` from the maker on `chain` for the taker's `request`:
    /// escrows the RWA and sends the fill. The taker pays `fill_fee` in XLM,
    /// and for CROSS_DELIVER the RWA's bridge fee too.
    /// `signature` is the maker's, in its chain's scheme.
    pub fn fill(env: Env, request: Request, chain: u32, bid: Bid, signature: Bytes) -> BytesN<32> {
        let cfg = Self::config(env.clone());
        let taker = request
            .taker
            .clone()
            .unwrap_or_else(|| panic_with_error!(&env, Error::NotAuthorized));
        taker.require_auth();
        let id = RfqClient::new(&env, &cfg.rfq).hash_request(&request);
        Self::check_request(&env, &request, &id);
        Self::check_bid(&env, &request, chain, &bid);
        let unit = Self::unit(&env, &request.maker_token);
        let tip = cfg.tip * unit;
        let quote = Self::quote(&env, &cfg, &request, &bid, tip);
        let order = Order {
            taker: taker.clone(),
            chain,
            rwa: request.taker_token.clone(),
            units: request.taker_amount,
            stable: request.maker_token.clone(),
            amount: quote.maker_amount,
            fee: quote.fee,
            tip,
            fee_recipient: request.fee_recipient.clone(),
            delivery: bid.delivery,
            recipient: wire::address(&env, &bid.recipient, bid.recipient_is_contract),
            maker: bid.maker.clone(),
            maker_is_contract: bid.maker_is_contract,
        };
        Self::put(&env, &Key::Filled(id.clone()), &true);
        Self::put(&env, &Key::Order(id.clone()), &order);

        let me = env.current_contract_address();
        token::Client::new(&env, &request.taker_token).transfer(&taker, &me, &request.taker_amount);
        let amount = (quote.maker_amount + unit - 1) / unit;
        let mut xlm = Self::peer_of(&env, chain).return_fee;
        if bid.delivery == CROSS_DELIVER {
            let payout = wire::payout(&env, &bid.maker, bid.maker_is_contract);
            xlm += transport::bridge_fee(&env, &cfg, chain, &order.rwa, order.units, payout);
        }
        token::Client::new(&env, &cfg.xlm).transfer(&taker, &me, &xlm);
        let payload = wire::fill(&env, &id, amount, &bid, &signature);
        transport::send(&env, &cfg, chain, payload, &taker);
        env.events().publish(
            (symbol_short!("filled"), id.clone(), chain),
            (quote.maker_amount, quote.fee),
        );
        id
    }

    /// The XLM `fill` charges for `chain`: the outbound message and the return leg.
    pub fn fill_fee(env: Env, chain: u32) -> i128 {
        let cfg = Self::config(env.clone());
        let mut payload = Bytes::new(&env);
        payload.extend_from_array(&[0u8; (wire::SIG_AT + 96) as usize]);
        transport::fee(&env, &cfg, chain, payload) + Self::peer_of(&env, chain).return_fee
    }

    /// Once a REDEEM_PUSH batch is paid, claims the proceeds and bridges them to
    /// the maker. Anyone may call; `caller` pays the bridge.
    pub fn push(env: Env, caller: Address, id: BytesN<32>) -> i128 {
        caller.require_auth();
        let cfg = Self::config(env.clone());
        let p = Self::push_of(env.clone(), id.clone())
            .unwrap_or_else(|| panic_with_error!(&env, Error::NotFound));
        env.storage().persistent().remove(&Key::Push(id.clone()));
        let me = env.current_contract_address();
        let (stable, rwa) = (
            token::Client::new(&env, &p.stable),
            token::Client::new(&env, &p.rwa),
        );
        let (before, rwa_before) = (stable.balance(&me), rwa.balance(&me));
        VaultClient::new(&env, &p.vault).claim(&me, &p.batch, &p.units, &me);
        Self::send_or_hold(&env, &p.rwa, &p.recipient, rwa.balance(&me) - rwa_before);
        let paid = stable.balance(&me) - before;
        let amount = paid - paid % Self::unit(&env, &p.stable);
        let payload = wire::payout(&env, &p.maker, p.maker_is_contract);
        transport::bridge(&env, &cfg, p.chain, &p.stable, amount, payload, &caller);
        env.events().publish((symbol_short!("pushed"), id), amount);
        amount
    }

    // ---- maker here

    /// The hash a maker signs (SEP-53) for `bid` on request `request_hash` on
    /// `taker_chain`: sha256 of the request hash, the chain and the bid's words.
    pub fn hash_bid(env: Env, request_hash: BytesN<32>, taker_chain: u32, bid: Bid) -> BytesN<32> {
        Self::bid_hash(
            &env,
            &request_hash,
            taker_chain,
            &wire::bid_words(&env, &bid),
        )
    }

    // ---- either

    /// Pays out what this contract owes `recipient` in `token` (a transfer
    /// that failed earlier: an issuer allowlist, a missing trustline).
    pub fn withdraw(env: Env, recipient: Address, token: Address) {
        recipient.require_auth();
        let k = Key::Owed(recipient.clone(), token.clone());
        let amount: i128 = env.storage().persistent().get(&k).unwrap_or(0);
        env.storage().persistent().remove(&k);
        token::Client::new(&env, &token).transfer(
            &env.current_contract_address(),
            &recipient,
            &amount,
        );
    }

    // ---- internals

    fn check_request(env: &Env, request: &Request, id: &BytesN<32>) {
        if request.sender != Some(env.current_contract_address())
            || request.order_type != OrderType::Rfq
        {
            panic_with_error!(env, Error::NotAuthorized);
        }
        if request.expiry <= env.ledger().timestamp() {
            panic_with_error!(env, Error::Expired);
        }
        if Self::filled(env.clone(), id.clone()) {
            panic_with_error!(env, Error::AlreadyFilled);
        }
    }

    /// A REDEEM_PUSH vault must pay out in the fill's stable, the one `push`
    /// bridges. CROSS_DELIVER needs a route for the RWA.
    fn check_bid(env: &Env, request: &Request, chain: u32, bid: &Bid) {
        if bid.deadline < env.ledger().timestamp() {
            panic_with_error!(env, Error::Expired);
        }
        if Self::route(env.clone(), chain, request.maker_token.clone()) != Some(bid.stable.clone())
        {
            panic_with_error!(env, Error::NoRoute);
        }
        if bid.delivery == CROSS_DELIVER {
            let rwa = request.taker_token.clone();
            return Self::route(env.clone(), chain, rwa)
                .map_or_else(|| panic_with_error!(env, Error::NoRoute), |_| ());
        }
        if bid.delivery > CROSS_DELIVER || bid.recipient == BytesN::from_array(env, &[0; 32]) {
            panic_with_error!(env, Error::InvalidBid);
        }
        if bid.delivery == DELIVER {
            return;
        }
        let vault = Self::vault(env.clone(), request.taker_token.clone())
            .unwrap_or_else(|| panic_with_error!(env, Error::NoVault));
        let paid_in = VaultClient::new(env, &vault).init().stable;
        if bid.delivery == REDEEM_PUSH && paid_in != request.maker_token {
            panic_with_error!(env, Error::NoVault);
        }
    }

    /// Prices the request at the maker's rate with the RFQ contract's oracle and schedule.
    fn quote(env: &Env, cfg: &Config, request: &Request, bid: &Bid, tip: i128) -> Quote {
        let cap = bid.max_amount * Self::unit(env, &request.maker_token);
        let order = RfqOrder {
            maker_token: request.maker_token.clone(),
            taker_token: request.taker_token.clone(),
            taker_amount: request.taker_amount,
            min_received_amount: request.min_received_amount,
            fee_bps: request.fee_bps,
            taker: request.taker.clone(),
            sender: request.sender.clone(),
            fee_recipient: request.fee_recipient.clone(),
            expiry: request.expiry,
            salt: request.salt,
            taker_max_bps_per_day: request.taker_max_bps_per_day,
            maker_bps_per_day: bid.maker_bps_per_day,
            max_maker_amount: cap,
            maker: env.current_contract_address(),
        };
        let q = RfqClient::new(env, &cfg.rfq).quote_rfq_order(&order, &request.taker_amount);
        if q.maker_amount > cap || q.maker_amount - q.fee - tip < request.min_received_amount {
            panic_with_error!(env, Error::QuoteRejected);
        }
        q
    }

    /// FILL from the peer on `chain`, maker here: pays, or sends FAIL.
    fn on_fill(env: &Env, cfg: &Config, chain: u32, message: &Bytes) {
        if let Err(e) = Self::pay(env, cfg, chain, message) {
            let id = wire::word(message, 1);
            env.events()
                .publish((symbol_short!("failed"), id.clone()), e as u32);
            let me = env.current_contract_address();
            transport::send(env, cfg, chain, wire::about(env, FAIL, &id), &me);
        }
    }

    /// FILL's work, maker here. An `Err` changes nothing and turns into FAIL.
    fn pay(env: &Env, cfg: &Config, chain: u32, message: &Bytes) -> Result<(), Error> {
        let bid = wire::parse_bid(message).ok_or(Error::InvalidBid)?;
        let id = wire::word(message, 1);
        let amount = wire::tail(&wire::word(message, 2)) as i128;
        let words = message.slice(wire::BID_AT..wire::SIG_AT);
        let hash = Self::bid_hash(env, &id, chain, &words);
        let maker = wire::address(env, &bid.maker, bid.maker_is_contract);
        let stable = wire::address(env, &bid.stable, true);
        let salt = wire::tail(&bid.salt) as u64;
        let bad = amount > bid.max_amount
            || bid.deadline < env.ledger().timestamp()
            || env.storage().persistent().has(&Key::Used(hash.clone()))
            || Self::route(env.clone(), chain, stable.clone()).is_none()
            || RfqClient::new(env, &cfg.rfq).is_salt_cancelled(&maker, &salt);
        if bad {
            return Err(Error::InvalidBid);
        }
        let sig = message.slice(wire::SIG_AT..);
        Self::check_signed(env, cfg, &maker, &bid, &hash, &sig)?;

        let me = env.current_contract_address();
        let local = amount * Self::unit(env, &stable);
        let pulled = token::Client::new(env, &stable).try_transfer_from(&me, &maker, &me, &local);
        if !matches!(pulled, Ok(Ok(()))) {
            return Err(Error::NotAuthorized);
        }
        Self::put(env, &Key::Used(hash.clone()), &true);
        let payload = wire::about(env, PAID, &id);
        transport::bridge(env, cfg, chain, &stable, local, payload, &me);
        env.events()
            .publish((symbol_short!("paid"), id, hash), local);
        Ok(())
    }

    /// The maker signed `hash` (SEP-53): with its own key, or one registered for
    /// it on the RFQ contract. `sig` is the signer's key then the signature.
    fn check_signed(
        env: &Env,
        cfg: &Config,
        maker: &Address,
        bid: &Bid,
        hash: &BytesN<32>,
        sig: &Bytes,
    ) -> Result<(), Error> {
        if sig.len() != 96 {
            return Err(Error::NotAuthorized);
        }
        let signer: BytesN<32> = sig.slice(0..32).try_into().unwrap();
        let signature: BytesN<64> = sig.slice(32..96).try_into().unwrap();
        let mut digest = Bytes::from_slice(env, b"Stellar Signed Message:\n");
        digest.extend_from_array(&hash.to_array());
        let digest = Bytes::from_array(env, &env.crypto().sha256(&digest).to_array());
        let valid = VerifierClient::new(env, &cfg.verifier)
            .try_verify(&signer, &digest, &signature)
            .is_ok();
        let own = !bid.maker_is_contract && bid.maker == signer;
        if !valid || (!own && !RfqClient::new(env, &cfg.rfq).is_order_signer(maker, &signer)) {
            return Err(Error::NotAuthorized);
        }
        Ok(())
    }

    /// Funds from the peer on `chain`: PAID settles a fill, PAYOUT is
    /// forwarded. `relayer`, if any, completed the delivery and takes the tip.
    fn on_funds(
        env: &Env,
        cfg: &Config,
        chain: u32,
        token: &Address,
        amount: i128,
        payload: &Bytes,
        relayer: Option<Address>,
    ) {
        let kind = wire::kind(payload);
        if kind == PAYOUT {
            let to_is_contract = wire::tail(&wire::word(payload, 2)) != 0;
            let to = wire::address(env, &wire::word(payload, 1), to_is_contract);
            let tip = relayer
                .as_ref()
                .map_or(0, |_| (cfg.tip * Self::unit(env, token)).min(amount));
            Self::send_or_hold(env, token, &relayer.unwrap_or(to.clone()), tip);
            return Self::send_or_hold(env, token, &to, amount - tip);
        }
        let id = wire::word(payload, 1);
        let o = Self::order(env.clone(), id.clone())
            .filter(|o| kind == PAID && o.chain == chain && &o.stable == token)
            .unwrap_or_else(|| panic_with_error!(env, Error::NotFound));
        if amount < o.amount {
            panic_with_error!(env, Error::Underpaid);
        }
        env.storage().persistent().remove(&Key::Order(id.clone()));
        Self::send_or_hold(env, token, &o.taker, amount - o.fee - o.tip);
        Self::send_or_hold(env, token, &o.fee_recipient, o.fee);
        Self::send_or_hold(env, token, &relayer.unwrap_or(o.taker.clone()), o.tip);
        Self::route_rwa(env, cfg, &id, &o);
        env.events()
            .publish((symbol_short!("settled"), id), (amount, o.fee, o.tip));
    }

    fn on_fail(env: &Env, chain: u32, id: &BytesN<32>) {
        let o = Self::order(env.clone(), id.clone())
            .filter(|o| o.chain == chain)
            .unwrap_or_else(|| panic_with_error!(env, Error::NotFound));
        env.storage().persistent().remove(&Key::Order(id.clone()));
        env.storage().persistent().remove(&Key::Filled(id.clone()));
        Self::send_or_hold(env, &o.rwa, &o.taker, o.units);
        env.events()
            .publish((symbol_short!("cancelled"), id.clone()), o.units);
    }

    /// The vault takes the RWA from us one call deeper, so we authorise that
    /// transfer. Its claim token comes here first, so a recipient the CT's gate
    /// refuses is owed it rather than blocking the settlement. CROSS_DELIVER's
    /// bridge must succeed, or the settlement reverts and is retried.
    fn route_rwa(env: &Env, cfg: &Config, id: &BytesN<32>, o: &Order) {
        if o.delivery == DELIVER {
            return Self::send_or_hold(env, &o.rwa, &o.recipient, o.units);
        }
        let me = env.current_contract_address();
        if o.delivery == CROSS_DELIVER {
            let payout = wire::payout(env, &o.maker, o.maker_is_contract);
            return transport::bridge(env, cfg, o.chain, &o.rwa, o.units, payout, &me);
        }
        let addr = Self::vault(env.clone(), o.rwa.clone())
            .unwrap_or_else(|| panic_with_error!(env, Error::NoVault));
        let vault = VaultClient::new(env, &addr);
        let pull = (me.clone(), addr.clone(), o.units).into_val(env);
        env.authorize_as_current_contract(vec![env, Self::auth(env, &o.rwa, "transfer", pull)]);
        if o.delivery == REDEEM_CT {
            let batch = vault.request_redeem(&me, &o.units, &me);
            return Self::send_or_hold(env, &vault.ct_of(&batch), &o.recipient, o.units);
        }
        let push = Push {
            vault: addr,
            rwa: o.rwa.clone(),
            recipient: o.recipient.clone(),
            batch: vault.request_redeem(&me, &o.units, &me),
            units: o.units,
            chain: o.chain,
            stable: o.stable.clone(),
            maker: o.maker.clone(),
            maker_is_contract: o.maker_is_contract,
        };
        Self::put(env, &Key::Push(id.clone()), &push);
    }

    fn auth(
        env: &Env,
        contract: &Address,
        fn_name: &str,
        args: soroban_sdk::Vec<Val>,
    ) -> InvokerContractAuthEntry {
        InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: contract.clone(),
                fn_name: Symbol::new(env, fn_name),
                args,
            },
            sub_invocations: vec![env],
        })
    }

    fn bid_hash(env: &Env, request_hash: &BytesN<32>, chain: u32, words: &Bytes) -> BytesN<32> {
        let mut b = Bytes::from_array(env, &request_hash.to_array());
        b.extend_from_array(&wire::uint(env, chain as u128).to_array());
        b.append(words);
        env.crypto().sha256(&b).to_bytes()
    }

    fn check_peer(env: &Env, chain: u32, sender: &BytesN<32>) {
        if Self::peer_of(env, chain).addr != *sender {
            panic_with_error!(env, Error::UnknownSender);
        }
    }

    fn peer_of(env: &Env, chain: u32) -> Peer {
        Self::peer(env.clone(), chain).unwrap_or_else(|| panic_with_error!(env, Error::NoPeer))
    }

    /// Units of `token` per unit on the wire, which has 6 decimals.
    fn unit(env: &Env, token: &Address) -> i128 {
        10i128.pow(token::Client::new(env, token).decimals() - 6)
    }

    /// Pays `token` to `to`, or owes it for `withdraw` if `to` cannot receive it yet.
    fn send_or_hold(env: &Env, token: &Address, to: &Address, amount: i128) {
        let me = env.current_contract_address();
        if amount <= 0
            || token::Client::new(env, token)
                .try_transfer(&me, to, &amount)
                .is_ok()
        {
            return;
        }
        let k = Key::Owed(to.clone(), token.clone());
        let owed: i128 = env.storage().persistent().get(&k).unwrap_or(0);
        Self::put(env, &k, &(owed + amount));
    }

    /// A route opens only where the bridge carries `stable` to `remote`.
    fn check_change(env: &Env, change: &Change) {
        if let Change::Route(chain, stable, Some(remote)) = change {
            transport::check_route(env, &Self::config(env.clone()), *chain, stable, remote);
        }
    }

    fn set_or_remove<V: IntoVal<Env, Val>>(env: &Env, k: &Key, v: Option<V>) {
        match v {
            Some(v) => env.storage().instance().set(k, &v),
            None => env.storage().instance().remove(k),
        }
    }

    fn admin(env: &Env) -> Address {
        env.storage().instance().get(&Key::Admin).unwrap()
    }

    fn put<V: IntoVal<Env, Val>>(env: &Env, k: &Key, v: &V) {
        env.storage().persistent().set(k, v);
        env.storage().persistent().extend_ttl(k, THRESHOLD, EXTEND);
        env.storage().instance().extend_ttl(THRESHOLD, EXTEND);
    }
}
