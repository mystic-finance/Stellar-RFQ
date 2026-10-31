//! `cctp_settle`: CCTP V2's transmitter and messenger, doubled as one contract.
use super::*;
use transport::Bridge;

/// Ethereum's CCTP domain, and its USDC there.
pub const CHAIN: u32 = 0;
pub const REMOTE: u128 = 0xC0C0;
pub const TIP: i128 = 500_000; // 0.50 USDC on the wire, 5_000_000 here
/// CCTP charges nothing at standard finality.
pub const MSG_FEE: i128 = 0;
pub const BRIDGE_FEE: i128 = 0;

mod mock_cctp {
    use super::*;

    /// CCTP V2 on Stellar, doubled: `receive_message` checks the destination
    /// caller and the nonce and mints `(amount - fee) x 10`; `send_message` and
    /// `deposit_for_burn_with_hook` (which pulls the burn) record.
    #[contract]
    pub struct MockCctp;

    #[contractimpl]
    impl MockCctp {
        pub fn init(env: Env, usdc: Address) {
            env.storage().instance().set(&0u32, &usdc);
            let remote = wire::uint(&env, REMOTE);
            env.storage().instance().set(&(CHAIN, remote), &usdc);
        }

        pub fn get_local_token(
            env: Env,
            remote_domain: u32,
            remote_token: BytesN<32>,
        ) -> Option<Address> {
            env.storage().instance().get(&(remote_domain, remote_token))
        }

        pub fn receive_message(
            env: Env,
            caller: Address,
            message: Bytes,
            _attestation: Bytes,
        ) -> bool {
            caller.require_auth();
            assert_eq!(
                message.slice(108..140),
                Bytes::from_array(&env, &wire::key(&env, &caller).to_array())
            );
            let nonce = env.crypto().sha256(&message).to_bytes();
            assert!(!env.storage().persistent().has(&nonce), "nonce used");
            env.storage().persistent().set(&nonce, &true);
            let amount = |at: u32| wire::tail(&wire::word_at(&message, at)) as i128;
            let recipient = wire::address(&env, &wire::word_at(&message, 184), true);
            let usdc: Address = env.storage().instance().get(&0u32).unwrap();
            StellarAssetClient::new(&env, &usdc)
                .mint(&recipient, &((amount(216) - amount(312)) * 10));
            true
        }

        pub fn send_message(
            env: Env,
            caller: Address,
            destination_domain: u32,
            recipient: BytesN<32>,
            destination_caller: BytesN<32>,
            min_finality_threshold: u32,
            message_body: Bytes,
        ) {
            caller.require_auth();
            let anyone = BytesN::from_array(&env, &[0; 32]);
            assert_eq!((destination_caller, min_finality_threshold), (anyone, 2000));
            let mut sent = Self::sent(env.clone());
            sent.push_back((destination_domain, recipient, message_body));
            env.storage().instance().set(&2u32, &sent);
        }

        pub fn deposit_for_burn_with_hook(
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
        ) {
            caller.require_auth();
            assert_eq!(
                mint_recipient, destination_caller,
                "the peer receives its own burns"
            );
            assert_eq!((max_fee, min_finality_threshold), (0, 2000));
            let me = env.current_contract_address();
            token::Client::new(&env, &burn_token).transfer_from(&me, &caller, &me, &amount);
            let mut burns = Self::burns(env.clone());
            burns.push_back((destination_domain, mint_recipient, amount, hook_data));
            env.storage().instance().set(&1u32, &burns);
        }

        pub fn burns(env: Env) -> Vec<(u32, BytesN<32>, i128, Bytes)> {
            env.storage()
                .instance()
                .get(&1u32)
                .unwrap_or(soroban_sdk::vec![&env])
        }

        pub fn sent(env: Env) -> Vec<(u32, BytesN<32>, Bytes)> {
            env.storage()
                .instance()
                .get(&2u32)
                .unwrap_or(soroban_sdk::vec![&env])
        }
    }
}
use mock_cctp::*;

pub struct Mocks {
    cctp: MockCctpClient<'static>,
}

/// CCTP and the USDC it mints, revocable like Circle's.
pub fn mocks(env: &Env, _xlm: &Address) -> (Mocks, Address, Bridge) {
    let cctp = MockCctpClient::new(env, &env.register(MockCctp, ()));
    let usdc = sac(env, &cctp.address);
    cctp.init(&usdc);
    let bridge = Bridge {
        transmitter: cctp.address.clone(),
        messenger: cctp.address.clone(),
    };
    (Mocks { cctp }, usdc, bridge)
}

impl F {
    /// A relayer receives the peer's message on the transmitter, which hands
    /// it to us; a FILL then runs.
    pub fn deliver(&self, message: &Bytes) {
        self.settle
            .handle_recv_finalized_message(&CHAIN, &self.peer, &2000, message);
        if wire::kind(message) == wire::FILL {
            self.settle.execute(message);
        }
    }

    pub fn try_pay_in(&self, amount_6dp: u128, payload: &Bytes, relayer: &Address) -> Try {
        let m = self.cctp_message(&self.peer, REMOTE, amount_6dp, payload);
        self.settle
            .try_complete(relayer, &m, &Bytes::new(&self.env))
    }

    /// The attested CCTP V2 message carrying `amount_6dp` of `token` and
    /// `hook`, burned by `sender`.
    fn cctp_message(
        &self,
        sender: &BytesN<32>,
        token: u128,
        amount_6dp: u128,
        hook: &Bytes,
    ) -> Bytes {
        let e = &self.env;
        let me = wire::key(e, &self.settle.address);
        let messenger = wire::key(e, &self.mocks.cctp.address);
        self.nonce.set(self.nonce.get() + 1);
        let mut m = Bytes::new(e);
        let u32s = |m: &mut Bytes, v: u32| m.extend_from_array(&v.to_be_bytes());
        u32s(&mut m, 1);
        u32s(&mut m, CHAIN);
        u32s(&mut m, 27);
        m.extend_from_array(&word_u(e, self.nonce.get() as u128).to_array());
        m.extend_from_array(&messenger.to_array()); // the remote messenger, in truth
        m.extend_from_array(&messenger.to_array());
        m.extend_from_array(&me.to_array()); // destinationCaller
        u32s(&mut m, 2000);
        u32s(&mut m, 2000);
        u32s(&mut m, 1);
        m.extend_from_array(&word_u(e, token).to_array()); // burnToken
        m.extend_from_array(&me.to_array()); // mintRecipient
        m.extend_from_array(&word_u(e, amount_6dp).to_array());
        m.extend_from_array(&sender.to_array()); // messageSender
        for _ in 0..3 {
            m.extend_from_array(&[0u8; 32]);
        }
        m.append(hook);
        m
    }

    /// Messages to the peer.
    pub fn sent(&self) -> std::vec::Vec<Bytes> {
        let sent = self.mocks.cctp.sent().iter();
        sent.inspect(|(domain, to, _)| assert_eq!((*domain, to), (CHAIN, &self.peer)))
            .map(|(_, _, body)| body)
            .collect()
    }

    /// Burns: domain, mint recipient, amount, hook.
    pub fn bridged(&self) -> std::vec::Vec<(u32, BytesN<32>, i128, Bytes)> {
        self.mocks.cctp.burns().iter().collect()
    }
}

#[test]
fn only_our_peer_delivers_through_the_transmitter() {
    let f = setup();
    let fail = wire::about(&f.env, FAIL, &word_u(&f.env, 1));
    let bad = word_u(&f.env, 0xBAD);
    let e = f
        .settle
        .try_handle_recv_finalized_message(&CHAIN, &bad, &2000, &fail);
    assert_eq!(e.unwrap_err().unwrap(), Error::UnknownSender.into());

    let paid = wire::about(&f.env, PAID, &word_u(&f.env, 1));
    let forged = f.cctp_message(&bad, REMOTE, 1, &paid);
    let e = f
        .settle
        .try_complete(&f.taker, &forged, &Bytes::new(&f.env));
    assert_eq!(e.unwrap_err().unwrap(), Error::UnknownSender.into());
    let mut not_a_burn = f.cctp_message(&f.peer, REMOTE, 1, &paid);
    not_a_burn.copy_from_slice(76, &bad.to_array());
    let e = f
        .settle
        .try_complete(&f.taker, &not_a_burn, &Bytes::new(&f.env));
    assert_eq!(e.unwrap_err().unwrap(), Error::UnknownSender.into());
    let unlinked = f.cctp_message(&f.peer, 0xBAD, 1, &paid);
    let e = f
        .settle
        .try_complete(&f.taker, &unlinked, &Bytes::new(&f.env));
    assert_eq!(e.unwrap_err().unwrap(), Error::NoRoute.into());

    // Only the transmitter hands us messages: here, nobody signs for it.
    let id = f.fill(&f.bid(DELIVER, &f.taker));
    let fail = wire::about(&f.env, FAIL, &id);
    f.env.set_auths(&[]);
    let e = f
        .settle
        .try_handle_recv_finalized_message(&CHAIN, &f.peer, &2000, &fail);
    assert!(e.is_err(), "unauthorized");
    assert!(f.settle.order(&id).is_some());
}

#[test]
fn a_fill_waits_for_execute_and_runs_once() {
    let f = setup();
    let id = word_u(&f.env, 0x1D);
    let m = f.fill_message(&f.key, &id, 992_500_000);
    f.settle
        .handle_recv_finalized_message(&CHAIN, &f.peer, &2000, &m);
    assert_eq!(
        f.bridged().len(),
        0,
        "its reply cannot reenter the transmitter"
    );
    f.settle.execute(&m);
    assert_eq!(f.bridged().len(), 1);
    assert_eq!(f.settle.try_execute(&m), Err(Ok(Error::NotFound.into())));
}

#[test]
fn a_burn_completes_once() {
    let f = setup();
    let id = f.fill(&f.bid(DELIVER, &f.taker));
    let m = f.cctp_message(
        &f.peer,
        REMOTE,
        992_500_000,
        &wire::about(&f.env, PAID, &id),
    );
    f.settle.complete(&f.taker, &m, &Bytes::new(&f.env));
    assert!(f
        .settle
        .try_complete(&f.taker, &m, &Bytes::new(&f.env))
        .is_err());
}
