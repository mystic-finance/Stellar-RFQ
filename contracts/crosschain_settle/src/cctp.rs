//! `cctp_settle`'s transport, Circle CCTP V2 on Stellar (circlefin/stellar-cctp):
//! messages through the MessageTransmitter, payments as burns through the
//! TokenMessengerMinter, for any token CCTP links. Chain ids are CCTP domains.
#![allow(clippy::too_many_arguments)]

use soroban_sdk::{
    contractclient, contractimpl, contracttype, panic_with_error, token, Address, Bytes, BytesN,
    Env,
};

use crate::{
    wire, Config, CrossChainSettle, CrossChainSettleArgs, CrossChainSettleClient, Error, Key,
};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Bridge {
    pub transmitter: Address,
    pub messenger: Address,
}

#[contractclient(name = "TransmitterClient")]
pub trait MessageTransmitter {
    fn send_message(
        env: Env,
        caller: Address,
        destination_domain: u32,
        recipient: BytesN<32>,
        destination_caller: BytesN<32>,
        min_finality_threshold: u32,
        message_body: Bytes,
    );
    fn receive_message(env: Env, caller: Address, message: Bytes, attestation: Bytes) -> bool;
}

/// Pulls the burn with `transfer_from`, so the caller approves it first.
#[contractclient(name = "MessengerClient")]
pub trait TokenMessenger {
    fn deposit_for_burn_with_hook(
        env: Env,
        caller: Address,
        amount: i128,
        destination_domain: u32,
        mint_recipient: BytesN<32>,
        burn_token: Address,
        destination_caller: BytesN<32>,
        max_fee: i128,
        min_finality_threshold: u32,
        hook_data: Bytes,
    );
    fn get_local_token(env: Env, remote_domain: u32, remote_token: BytesN<32>) -> Option<Address>;
}

/// Standard finality: free, and attested in seconds from Stellar.
pub const FINALIZED: u32 = 2000;
/// Burn message body at 148: its `burnToken` at +4, `messageSender` at +100
/// and `hookData` at +228.
pub const HOOK: u32 = 376;
const SOURCE_DOMAIN: u32 = 4;
const RECIPIENT: u32 = 76;
const BURN_TOKEN: u32 = 152;
const MESSAGE_SENDER: u32 = 248;

/// Where a message came from: its CCTP domain and the contract that burned.
pub fn origin(message: &Bytes) -> Option<(u32, BytesN<32>)> {
    (message.len() > HOOK).then(|| {
        let mut domain = [0u8; 4];
        message
            .slice(SOURCE_DOMAIN..SOURCE_DOMAIN + 4)
            .copy_into_slice(&mut domain);
        (
            u32::from_be_bytes(domain),
            wire::word_at(message, MESSAGE_SENDER),
        )
    })
}

pub fn init(_env: &Env, _cfg: &Config, _admin: Address) {}

pub fn fee(_env: &Env, _cfg: &Config, _chain: u32, _payload: Bytes) -> i128 {
    0
}

/// CCTP carries no RWA, so a CROSS_DELIVER route never opens.
/// CCTP burns only Circle's tokens, so CROSS_DELIVER is refused at fill whatever the routes say.
pub fn bridge_fee(env: &Env, _: &Config, _: u32, _: &Address, _: i128, _: Bytes) -> i128 {
    panic_with_error!(env, Error::NoRoute)
}

/// Sends `payload` to the peer on domain `chain`, for anyone to relay.
pub fn send(env: &Env, cfg: &Config, chain: u32, payload: Bytes, _payer: &Address) {
    let peer = CrossChainSettle::peer_of(env, chain);
    let anyone = BytesN::from_array(env, &[0; 32]);
    TransmitterClient::new(env, &cfg.bridge.transmitter).send_message(
        &env.current_contract_address(),
        &chain,
        &peer.addr,
        &anyone,
        &FINALIZED,
        &payload,
    );
}

/// Burns `amount` of `stable` to the peer on domain `chain`, `payload` its
/// hook. Only the peer may receive it, so the hook always runs.
pub fn bridge(
    env: &Env,
    cfg: &Config,
    chain: u32,
    stable: &Address,
    amount: i128,
    payload: Bytes,
    _payer: &Address,
) {
    let peer = CrossChainSettle::peer_of(env, chain);
    let me = env.current_contract_address();
    let sequence = env.ledger().sequence();
    token::Client::new(env, stable).approve(&me, &cfg.bridge.messenger, &amount, &sequence);
    MessengerClient::new(env, &cfg.bridge.messenger).deposit_for_burn_with_hook(
        &me, &amount, &chain, &peer.addr, stable, &peer.addr, &0, &FINALIZED, &payload,
    );
}

/// A route needs CCTP to link `stable` with `remote` on domain `chain`.
pub fn check_route(env: &Env, cfg: &Config, chain: u32, stable: &Address, remote: &BytesN<32>) {
    let local = MessengerClient::new(env, &cfg.bridge.messenger).get_local_token(&chain, remote);
    if local.as_ref() != Some(stable) {
        panic_with_error!(env, Error::NoRoute);
    }
}

#[contractimpl]
impl CrossChainSettle {
    /// FILL or FAIL from the peer on `source_domain`, which the transmitter
    /// hands us when anyone `receive_message`s it. A FILL waits for `execute`:
    /// its reply goes through the transmitter, which is still on the stack.
    pub fn handle_recv_finalized_message(
        env: Env,
        source_domain: u32,
        sender: BytesN<32>,
        _finality_threshold_executed: u32,
        message_body: Bytes,
    ) -> bool {
        Self::config(env.clone()).bridge.transmitter.require_auth();
        Self::check_peer(&env, source_domain, &sender);
        if wire::kind(&message_body) != wire::FILL {
            Self::on_fail(&env, source_domain, &wire::word(&message_body, 1));
            return true;
        }
        let k = Key::Inbox(env.crypto().sha256(&message_body).to_bytes());
        Self::put(&env, &k, &source_domain);
        true
    }

    /// Anyone runs a received FILL: pays, or sends FAIL.
    pub fn execute(env: Env, message: Bytes) {
        let k = Key::Inbox(env.crypto().sha256(&message).to_bytes());
        let chain: u32 = env
            .storage()
            .persistent()
            .get(&k)
            .unwrap_or_else(|| panic_with_error!(&env, Error::NotFound));
        env.storage().persistent().remove(&k);
        Self::on_fill(&env, &Self::config(env.clone()), chain, &message);
    }

    /// A payment: receives the burn our peer made, which mints here, settles or
    /// forwards with it and pays `caller` the tip. Only a burn: the messenger
    /// receives it, and vouches for its burner.
    pub fn complete(env: Env, caller: Address, message: Bytes, attestation: Bytes) {
        caller.require_auth();
        let cfg = Self::config(env.clone());
        let messenger = wire::key(&env, &cfg.bridge.messenger);
        let (chain, sender) = origin(&message)
            .filter(|_| wire::word_at(&message, RECIPIENT) == messenger)
            .unwrap_or_else(|| panic_with_error!(&env, Error::UnknownSender));
        Self::check_peer(&env, chain, &sender);
        let burned = wire::word_at(&message, BURN_TOKEN);
        let stable = MessengerClient::new(&env, &cfg.bridge.messenger)
            .get_local_token(&chain, &burned)
            .unwrap_or_else(|| panic_with_error!(&env, Error::NoRoute));
        let me = env.current_contract_address();
        let local = token::Client::new(&env, &stable);
        let before = local.balance(&me);
        TransmitterClient::new(&env, &cfg.bridge.transmitter).receive_message(
            &me,
            &message,
            &attestation,
        );
        let received = local.balance(&me) - before;
        let hook = message.slice(HOOK..);
        Self::on_funds(&env, &cfg, chain, &stable, received, &hook, Some(caller));
    }
}
