//! `lz_settle`: LayerZero's endpoint and an OFT, doubled.
use super::*;
use transport::{Bridge, MessagingFee, MessagingParams, MessagingReceipt, OftReceipt};
use transport::{Origin, SendParam};

pub const CHAIN: u32 = 30101;
/// The OFT's token on the peer's chain.
pub const REMOTE: u128 = 0x0707;
/// Messages and payments are delivered by LayerZero's executor, so no tip.
pub const TIP: i128 = 0;
pub const MSG_FEE: i128 = 100;
pub const BRIDGE_FEE: i128 = 50;

mod mock_endpoint {
    use super::*;

    /// LayerZero's endpoint, doubled: a flat fee it expects to receive before
    /// each `send`, which records; `clear` and `clear_compose` accept each payload once.
    #[contract]
    pub struct MockEndpoint;

    #[contractimpl]
    impl MockEndpoint {
        pub fn init(env: Env, xlm: Address) {
            env.storage().instance().set(&0u32, &xlm);
        }

        pub fn quote(_env: Env, _sender: Address, _params: MessagingParams) -> MessagingFee {
            MessagingFee {
                native_fee: MSG_FEE,
                zro_fee: 0,
            }
        }

        pub fn send(
            env: Env,
            sender: Address,
            params: MessagingParams,
            _refund_address: Address,
        ) -> MessagingReceipt {
            sender.require_auth();
            let xlm: Address = env.storage().instance().get(&0u32).unwrap();
            let mut sent = Self::sent(env.clone());
            let held = TokenClient::new(&env, &xlm).balance(&env.current_contract_address());
            let before: i128 = env.storage().instance().get(&2u32).unwrap_or(0);
            assert!(held - before >= MSG_FEE, "the fee arrives first");
            env.storage().instance().set(&2u32, &held);
            sent.push_back(params);
            env.storage().instance().set(&1u32, &sent);
            MessagingReceipt {
                guid: BytesN::from_array(&env, &[0; 32]),
                nonce: sent.len() as u64,
                fee: MessagingFee {
                    native_fee: MSG_FEE,
                    zro_fee: 0,
                },
            }
        }

        pub fn clear(
            env: Env,
            caller: Address,
            _origin: Origin,
            _receiver: Address,
            guid: BytesN<32>,
            message: Bytes,
        ) {
            caller.require_auth();
            Self::once(&env, guid, message);
        }

        pub fn clear_compose(
            env: Env,
            composer: Address,
            _from: Address,
            guid: BytesN<32>,
            _index: u32,
            message: Bytes,
        ) {
            composer.require_auth();
            Self::once(&env, guid, message);
        }

        pub fn set_delegate(_env: Env, oapp: Address, _new_delegate: Option<Address>) {
            oapp.require_auth();
        }

        pub fn sent(env: Env) -> Vec<MessagingParams> {
            env.storage()
                .instance()
                .get(&1u32)
                .unwrap_or(soroban_sdk::vec![&env])
        }

        fn once(env: &Env, guid: BytesN<32>, message: Bytes) {
            let mut b = Bytes::from_array(env, &guid.to_array());
            b.append(&message);
            let k = env.crypto().sha256(&b).to_bytes();
            assert!(!env.storage().persistent().has(&k), "payload cleared");
            env.storage().persistent().set(&k, &true);
        }
    }
}
use mock_endpoint::*;

mod mock_oft {
    use super::*;

    /// The OFT, doubled: burns from `from`, takes the fee from `from` to the
    /// endpoint, and records the send.
    #[contract]
    pub struct MockOft;

    #[contractimpl]
    impl MockOft {
        pub fn init(env: Env, token: Address, xlm: Address, endpoint: Address) {
            env.storage().instance().set(&0u32, &(token, xlm, endpoint));
        }

        pub fn quote_send(
            _env: Env,
            _from: Address,
            _send_param: SendParam,
            _pay_in_zro: bool,
        ) -> MessagingFee {
            MessagingFee {
                native_fee: BRIDGE_FEE,
                zro_fee: 0,
            }
        }

        pub fn send(
            env: Env,
            from: Address,
            send_param: SendParam,
            fee: MessagingFee,
            _refund_address: Address,
        ) -> (MessagingReceipt, OftReceipt) {
            from.require_auth();
            let (token, xlm, endpoint): (Address, Address, Address) =
                env.storage().instance().get(&0u32).unwrap();
            TokenClient::new(&env, &token).burn(&from, &send_param.amount_ld);
            TokenClient::new(&env, &xlm).transfer(&from, &endpoint, &fee.native_fee);
            let mut sent = Self::sent(env.clone());
            sent.push_back(send_param.clone());
            env.storage().instance().set(&1u32, &sent);
            let amount = send_param.amount_ld;
            (
                MessagingReceipt {
                    guid: BytesN::from_array(&env, &[0; 32]),
                    nonce: 1,
                    fee,
                },
                OftReceipt {
                    amount_sent_ld: amount,
                    amount_received_ld: amount,
                },
            )
        }

        pub fn sent(env: Env) -> Vec<SendParam> {
            env.storage()
                .instance()
                .get(&1u32)
                .unwrap_or(soroban_sdk::vec![&env])
        }
    }
}
use mock_oft::*;

pub struct Mocks {
    endpoint: MockEndpointClient<'static>,
    oft: MockOftClient<'static>,
    executor: Address,
}

/// The endpoint, the OFT and its token (USDT0, revocable like a permissioned stable).
pub fn mocks(env: &Env, xlm: &Address) -> (Mocks, Address, Bridge) {
    let endpoint = MockEndpointClient::new(env, &env.register(MockEndpoint, ()));
    let oft = MockOftClient::new(env, &env.register(MockOft, ()));
    let token = sac(env, &oft.address);
    endpoint.init(xlm);
    oft.init(&token, xlm, &endpoint.address);
    let bridge = Bridge {
        endpoint: endpoint.address.clone(),
        ofts: soroban_sdk::map![env, (token.clone(), oft.address.clone())],
    };
    let executor = Address::generate(env);
    (
        Mocks {
            endpoint,
            oft,
            executor,
        },
        token,
        bridge,
    )
}

impl F {
    pub fn deliver(&self, message: &Bytes) {
        self.nonce.set(self.nonce.get() + 1);
        let origin = Origin {
            src_eid: CHAIN,
            sender: self.peer.clone(),
            nonce: self.nonce.get() as u64,
        };
        let guid = BytesN::from_array(&self.env, &[self.nonce.get(); 32]);
        let none = Bytes::new(&self.env);
        let executor = &self.mocks.executor;
        self.settle
            .lz_receive(executor, &origin, &guid, message, &none, &0);
    }

    /// The OFT mints here and the executor composes `payload` (Stellar codec).
    pub fn try_pay_in(&self, amount_6dp: u128, payload: &Bytes, _relayer: &Address) -> Try {
        let oft = &self.mocks.oft.address;
        self.compose(oft, &self.stable, amount_6dp as i128 * 10, payload)
    }

    /// `oft` mints `amount` of `token` here and the executor composes `payload`.
    fn compose(&self, oft: &Address, token: &Address, amount: i128, payload: &Bytes) -> Try {
        StellarAssetClient::new(&self.env, token).mint(&self.settle.address, &amount);
        let mut m = Bytes::from_array(&self.env, &[0u8; 8]);
        m.extend_from_array(&CHAIN.to_be_bytes());
        m.extend_from_array(&amount.to_be_bytes());
        m.extend_from_array(&self.peer.to_array());
        m.append(payload);
        let guid = BytesN::from_array(&self.env, &[0xC0; 32]);
        let none = Bytes::new(&self.env);
        let executor = &self.mocks.executor;
        self.settle
            .try_lz_compose(executor, oft, &guid, &0, &m, &none, &0)
    }

    /// Messages to the peer.
    pub fn sent(&self) -> std::vec::Vec<Bytes> {
        let sent = self.mocks.endpoint.sent().iter();
        sent.inspect(|p| assert_eq!((p.dst_eid, &p.receiver), (CHAIN, &self.peer)))
            .map(|p| p.message)
            .collect()
    }

    /// Payments out: chain, receiver, amount, composed payload.
    pub fn bridged(&self) -> std::vec::Vec<(u32, BytesN<32>, i128, Bytes)> {
        let sent = self.mocks.oft.sent().iter();
        sent.map(|s| (s.dst_eid, s.to, s.amount_ld, s.compose_msg))
            .collect()
    }
}

#[test]
fn only_our_peer_and_our_oft_deliver() {
    let f = setup();
    let origin = Origin {
        src_eid: CHAIN,
        sender: word_u(&f.env, 0xBAD),
        nonce: 1,
    };
    let guid = BytesN::from_array(&f.env, &[1; 32]);
    let (none, executor) = (Bytes::new(&f.env), &f.mocks.executor);
    let msg = wire::about(&f.env, FAIL, &word_u(&f.env, 1));
    let e = f
        .settle
        .try_lz_receive(executor, &origin, &guid, &msg, &none, &0);
    assert_eq!(e.unwrap_err().unwrap(), Error::UnknownSender.into());

    let fake = Address::generate(&f.env);
    let e = f
        .settle
        .try_lz_compose(executor, &fake, &guid, &0, &msg, &none, &0);
    assert_eq!(e.unwrap_err().unwrap(), Error::UnknownSender.into());

    assert!(f.settle.allow_initialize_path(&Origin {
        src_eid: CHAIN,
        sender: f.peer.clone(),
        nonce: 0
    }));
    assert!(!f.settle.allow_initialize_path(&origin));
}

/// The config with an OFT for the RWA.
fn with_rwa_oft(f: &F) -> (Config, MockOftClient<'static>) {
    let oft = MockOftClient::new(&f.env, &f.env.register(MockOft, ()));
    oft.init(&f.rwa, &f.xlm, &f.mocks.endpoint.address);
    let mut cfg = f.settle.config();
    cfg.bridge.ofts.set(f.rwa.clone(), oft.address.clone());
    (cfg, oft)
}

fn rwa_route(f: &F) -> Change {
    Change::Route(CHAIN, f.rwa.clone(), Some(word_u(&f.env, 0x8A)))
}

/// The RWA's OFT registered and its route open.
fn rwa_oft(f: &F) -> MockOftClient<'static> {
    let (cfg, oft) = with_rwa_oft(f);
    govern(&f.settle, &Change::Config(cfg));
    govern(&f.settle, &rwa_route(f));
    oft
}

#[test]
fn an_oft_waits_out_the_timelock() {
    let f = setup();
    govern(&f.settle, &Change::Delay(DAY));
    f.settle.submit(&Change::Config(with_rwa_oft(&f).0));
    let refused = Err(Ok(Error::NoRoute.into()));
    assert_eq!(f.settle.try_submit(&rwa_route(&f)), refused);
    f.env
        .ledger()
        .set_timestamp(f.env.ledger().timestamp() + DAY);
    f.settle.accept(&0);
    f.settle.submit(&rwa_route(&f));
}

#[test]
fn cross_deliver_bridges_the_rwa_to_the_maker_home() {
    let f = setup();
    let oft = rwa_oft(&f);
    let mut bid = f.bid(CROSS_DELIVER, &f.taker);
    bid.recipient = BytesN::from_array(&f.env, &[0; 32]); // unused: the RWA goes to the maker
    let id = f.fill(&bid);
    assert_eq!(
        f.balance(&f.xlm, &f.taker),
        XLM - MSG_FEE - RETURN_FEE - BRIDGE_FEE,
        "the taker prepays the RWA's bridge"
    );

    let paid = wire::about(&f.env, PAID, &id);
    let held = f.balance(&f.xlm, &f.settle.address);
    f.settle.sweep(&f.operator, &held);
    assert!(f.try_pay_in(992_500_000, &paid, &f.taker).is_err());
    assert_eq!(f.balance(&f.rwa, &f.settle.address), UNITS, "nothing moved");
    StellarAssetClient::new(&f.env, &f.xlm).mint(&f.settle.address, &held);

    f.pay_in(992_500_000, &paid);
    let s = oft.sent().get(0).unwrap();
    let payout = wire::payout(&f.env, &word_u(&f.env, 0xA11CE), false);
    assert_eq!(
        (s.dst_eid, s.to, s.amount_ld, s.compose_msg),
        (CHAIN, f.peer.clone(), UNITS, payout)
    );
    assert_eq!(f.balance(&f.rwa, &f.settle.address), 0, "burned");
    assert_eq!(f.settle.order(&id), None);
}

#[test]
fn an_rwa_arriving_through_its_oft_is_forwarded_to_the_maker() {
    let f = setup();
    let oft = rwa_oft(&f);
    let to = wire::payout(&f.env, &wire::key(&f.env, &f.maker), true);
    f.compose(&oft.address, &f.rwa, UNITS, &to)
        .unwrap()
        .unwrap();
    assert_eq!(f.balance(&f.rwa, &f.maker), UNITS);
}
