//! `lz_settle`'s transport, LayerZero V2 on Stellar: messages through the
//! endpoint, tokens through their OFTs. Its calls are written against their
//! ScVal interface (the LayerZero crates need a newer soroban-sdk). Field names
//! and argument order are the wire format. Chain ids are EIDs.

use soroban_sdk::{
    contractclient, contractimpl, contracttype, panic_with_error, token, vec, Address, Bytes,
    BytesN, Env, IntoVal, Map,
};

use crate::{wire, Config, CrossChainSettle, CrossChainSettleArgs, CrossChainSettleClient, Error};

/// LayerZero's endpoint, and the OFT each token crosses through, by token.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Bridge {
    pub endpoint: Address,
    pub ofts: Map<Address, Address>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessagingParams {
    pub dst_eid: u32,
    pub receiver: BytesN<32>,
    pub message: Bytes,
    pub options: Bytes,
    pub pay_in_zro: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessagingFee {
    pub native_fee: i128,
    pub zro_fee: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessagingReceipt {
    pub guid: BytesN<32>,
    pub nonce: u64,
    pub fee: MessagingFee,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Origin {
    pub src_eid: u32,
    pub sender: BytesN<32>,
    pub nonce: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SendParam {
    pub dst_eid: u32,
    pub to: BytesN<32>,
    pub amount_ld: i128,
    pub min_amount_ld: i128,
    pub extra_options: Bytes,
    pub compose_msg: Bytes,
    pub oft_cmd: Bytes,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OftReceipt {
    pub amount_sent_ld: i128,
    pub amount_received_ld: i128,
}

/// The endpoint takes its fee as native XLM transferred to it before `send`.
#[contractclient(name = "EndpointClient")]
pub trait Endpoint {
    fn quote(env: Env, sender: Address, params: MessagingParams) -> MessagingFee;
    fn send(
        env: Env,
        sender: Address,
        params: MessagingParams,
        refund_address: Address,
    ) -> MessagingReceipt;
    fn clear(
        env: Env,
        caller: Address,
        origin: Origin,
        receiver: Address,
        guid: BytesN<32>,
        message: Bytes,
    );
    fn clear_compose(
        env: Env,
        composer: Address,
        from: Address,
        guid: BytesN<32>,
        index: u32,
        message: Bytes,
    );
    fn set_delegate(env: Env, oapp: Address, new_delegate: Option<Address>);
}

/// The OFT. `send` burns from `from` and pays the endpoint from `from`.
#[contractclient(name = "OftClient")]
pub trait Oft {
    fn quote_send(env: Env, from: Address, send_param: SendParam, pay_in_zro: bool)
        -> MessagingFee;
    fn send(
        env: Env,
        from: Address,
        send_param: SendParam,
        fee: MessagingFee,
        refund_address: Address,
    ) -> (MessagingReceipt, OftReceipt);
}

/// The admin becomes the OApp's delegate.
pub fn init(env: &Env, cfg: &Config, admin: Address) {
    EndpointClient::new(env, &cfg.bridge.endpoint)
        .set_delegate(&env.current_contract_address(), &Some(admin));
}

pub fn fee(env: &Env, cfg: &Config, chain: u32, payload: Bytes) -> i128 {
    let me = env.current_contract_address();
    let endpoint = EndpointClient::new(env, &cfg.bridge.endpoint);
    endpoint.quote(&me, &params(env, chain, payload)).native_fee
}

/// Sends `payload` to the peer on `chain`; `payer` pays the endpoint in XLM.
pub fn send(env: &Env, cfg: &Config, chain: u32, payload: Bytes, payer: &Address) {
    let me = env.current_contract_address();
    let endpoint = EndpointClient::new(env, &cfg.bridge.endpoint);
    let params = params(env, chain, payload);
    let fee = endpoint.quote(&me, &params);
    token::Client::new(env, &cfg.xlm).transfer(payer, &cfg.bridge.endpoint, &fee.native_fee);
    endpoint.send(&me, &params, &me);
}

/// The XLM `bridge` costs for the same arguments.
pub fn bridge_fee(
    env: &Env,
    cfg: &Config,
    chain: u32,
    token: &Address,
    amount: i128,
    payload: Bytes,
) -> i128 {
    let param = send_param(env, chain, amount, payload);
    let (me, oft) = (env.current_contract_address(), oft_of(env, cfg, token));
    oft.quote_send(&me, &param, &false).native_fee
}

/// Sends `amount` of `stable` through its OFT to the peer on `chain`,
/// composing `payload`; `payer` pays the OFT's fee in XLM.
pub fn bridge(
    env: &Env,
    cfg: &Config,
    chain: u32,
    stable: &Address,
    amount: i128,
    payload: Bytes,
    payer: &Address,
) {
    let me = env.current_contract_address();
    let param = send_param(env, chain, amount, payload);
    let oft = oft_of(env, cfg, stable);
    let fee = oft.quote_send(&me, &param, &false);
    if payer != &me {
        token::Client::new(env, &cfg.xlm).transfer(payer, &me, &fee.native_fee);
    }
    // The OFT burns from us and pays the endpoint from us, one call deeper.
    let burn = (me.clone(), amount).into_val(env);
    let pay = (me.clone(), cfg.bridge.endpoint.clone(), fee.native_fee).into_val(env);
    env.authorize_as_current_contract(vec![
        env,
        CrossChainSettle::auth(env, stable, "burn", burn),
        CrossChainSettle::auth(env, &cfg.xlm, "transfer", pay),
    ]);
    oft.send(&me, &param, &fee, &me);
}

/// A route carries a token with an OFT only.
pub fn check_route(env: &Env, cfg: &Config, _chain: u32, stable: &Address, _remote: &BytesN<32>) {
    if !cfg.bridge.ofts.contains_key(stable.clone()) {
        panic_with_error!(env, Error::NoRoute);
    }
}

fn oft_of<'a>(env: &'a Env, cfg: &Config, token: &Address) -> OftClient<'a> {
    let oft = cfg.bridge.ofts.get(token.clone());
    let oft = oft.unwrap_or_else(|| panic_with_error!(env, Error::NoRoute));
    OftClient::new(env, &oft)
}

fn send_param(env: &Env, chain: u32, amount: i128, payload: Bytes) -> SendParam {
    let peer = CrossChainSettle::peer_of(env, chain);
    SendParam {
        dst_eid: chain,
        to: peer.addr,
        amount_ld: amount,
        min_amount_ld: amount,
        extra_options: peer.options,
        compose_msg: payload,
        oft_cmd: Bytes::new(env),
    }
}

fn params(env: &Env, chain: u32, message: Bytes) -> MessagingParams {
    let peer = CrossChainSettle::peer_of(env, chain);
    MessagingParams {
        dst_eid: chain,
        receiver: peer.addr,
        message,
        options: peer.options,
        pay_in_zro: false,
    }
}

/// Native value the executor delivered with a message.
fn take_value(env: &Env, cfg: &Config, executor: &Address, value: i128) {
    if value != 0 {
        let me = env.current_contract_address();
        token::Client::new(env, &cfg.xlm).transfer(executor, &me, &value);
    }
}

#[contractimpl]
impl CrossChainSettle {
    /// LayerZero delivery of FILL or FAIL, called by the executor.
    pub fn lz_receive(
        env: Env,
        executor: Address,
        origin: Origin,
        guid: BytesN<32>,
        message: Bytes,
        _extra_data: Bytes,
        value: i128,
    ) {
        executor.require_auth();
        let cfg = Self::config(env.clone());
        Self::check_peer(&env, origin.src_eid, &origin.sender);
        let me = env.current_contract_address();
        EndpointClient::new(&env, &cfg.bridge.endpoint).clear(&me, &origin, &me, &guid, &message);
        take_value(&env, &cfg, &executor, value);
        if wire::kind(&message) != wire::FILL {
            return Self::on_fail(&env, origin.src_eid, &wire::word(&message, 1));
        }
        Self::on_fill(&env, &cfg, origin.src_eid, &message);
    }

    /// A payment: an OFT minted here and LayerZero's executor composes PAID or
    /// PAYOUT. The message is the Stellar OFTComposeMsgCodec's: nonce, source
    /// EID at 8, amount (i128) at 12, sender at 28, ours from 60.
    pub fn lz_compose(
        env: Env,
        executor: Address,
        from: Address,
        guid: BytesN<32>,
        index: u32,
        message: Bytes,
        _extra_data: Bytes,
        value: i128,
    ) {
        executor.require_auth();
        let cfg = Self::config(env.clone());
        let (token, _) = (cfg.bridge.ofts.iter())
            .find(|(_, oft)| oft == &from)
            .unwrap_or_else(|| panic_with_error!(&env, Error::UnknownSender));
        let me = env.current_contract_address();
        EndpointClient::new(&env, &cfg.bridge.endpoint)
            .clear_compose(&me, &from, &guid, &index, &message);
        take_value(&env, &cfg, &executor, value);
        let mut head = [0u8; 28];
        message.slice(0..28).copy_into_slice(&mut head);
        let eid = u32::from_be_bytes(head[8..12].try_into().unwrap());
        let amount = i128::from_be_bytes(head[12..28].try_into().unwrap());
        Self::check_peer(&env, eid, &wire::word_at(&message, 28));
        let payload = message.slice(60..);
        Self::on_funds(&env, &cfg, eid, &token, amount, &payload, None);
    }

    /// LayerZero opens a pathway only from a peer.
    pub fn allow_initialize_path(env: Env, origin: Origin) -> bool {
        Self::peer(env, origin.src_eid).is_some_and(|p| p.addr == origin.sender)
    }

    /// Messages are delivered in any order.
    pub fn next_nonce(_env: Env, _src_eid: u32, _sender: BytesN<32>) -> u64 {
        0
    }
}
