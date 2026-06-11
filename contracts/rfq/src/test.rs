#![cfg(test)]

use super::*;
use ed25519_dalek::{Signer as _, SigningKey};
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::{Address, BytesN, Env};

const HUGE: i128 = 1_000_000_000_000;

struct Fixture {
    env: Env,
    client: RfqContractClient<'static>,
    contract_id: Address,
    maker: Address,
    taker: Address,
    maker_token: Address,
    taker_token: Address,
    maker_key: SigningKey,
    maker_pubkey: BytesN<32>,
}

fn create_token(env: &Env, admin: &Address) -> Address {
    env.register_stellar_asset_contract_v2(admin.clone())
        .address()
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(RfqContract, ());
    let client = RfqContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let maker = Address::generate(&env);
    let taker = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let maker_token = create_token(&env, &token_admin);
    let taker_token = create_token(&env, &token_admin);

    StellarAssetClient::new(&env, &maker_token).mint(&maker, &HUGE);
    StellarAssetClient::new(&env, &taker_token).mint(&taker, &HUGE);

    // Pre-approve the settlement contract as spender, like makers/takers
    // approving the 0x Exchange Proxy.
    let exp = env.ledger().sequence() + 1_000_000;
    TokenClient::new(&env, &maker_token).approve(&maker, &contract_id, &HUGE, &exp);
    TokenClient::new(&env, &taker_token).approve(&taker, &contract_id, &HUGE, &exp);

    // The maker's off-chain signing key, registered on-chain.
    let maker_key = SigningKey::from_bytes(&[7u8; 32]);
    let maker_pubkey = BytesN::from_array(&env, &maker_key.verifying_key().to_bytes());
    client.register_order_signer(&maker, &maker_pubkey, &true);

    Fixture {
        env,
        client,
        contract_id,
        maker,
        taker,
        maker_token,
        taker_token,
        maker_key,
        maker_pubkey,
    }
}

fn sign(env: &Env, key: &SigningKey, hash: &BytesN<32>) -> Signature {
    let sig = key.sign(&hash.to_array());
    Signature {
        signer: BytesN::from_array(env, &key.verifying_key().to_bytes()),
        signature: BytesN::from_array(env, &sig.to_bytes()),
    }
}

impl Fixture {
    fn rfq_order(&self) -> RfqOrder {
        RfqOrder {
            maker_token: self.maker_token.clone(),
            taker_token: self.taker_token.clone(),
            maker_amount: 1_000,
            taker_amount: 2_000,
            maker: self.maker.clone(),
            taker: None,
            tx_origin: self.taker.clone(),
            pool: BytesN::from_array(&self.env, &[0u8; 32]),
            expiry: self.env.ledger().timestamp() + 1_000,
            salt: 1,
        }
    }

    fn limit_order(&self) -> LimitOrder {
        LimitOrder {
            maker_token: self.maker_token.clone(),
            taker_token: self.taker_token.clone(),
            maker_amount: 1_000,
            taker_amount: 2_000,
            token_fee_amount: 0,
            maker: self.maker.clone(),
            taker: None,
            sender: None,
            fee_recipient: self.maker.clone(),
            pool: BytesN::from_array(&self.env, &[0u8; 32]),
            expiry: self.env.ledger().timestamp() + 1_000,
            salt: 1,
        }
    }

    fn maker_token_balance(&self, who: &Address) -> i128 {
        TokenClient::new(&self.env, &self.maker_token).balance(who)
    }
    fn taker_token_balance(&self, who: &Address) -> i128 {
        TokenClient::new(&self.env, &self.taker_token).balance(who)
    }
}

#[test]
fn rfq_partial_then_full_fill() {
    let f = setup();
    let order = f.rfq_order();
    let hash = f.client.get_rfq_order_hash(&order);
    let sig = sign(&f.env, &f.maker_key, &hash);

    // Partial fill: 1000 of 2000 taker tokens -> 500 of 1000 maker tokens.
    let r = f.client.fill_rfq_order(&order, &sig, &f.taker, &1_000);
    assert_eq!(r.taker_token_filled_amount, 1_000);
    assert_eq!(r.maker_token_filled_amount, 500);
    assert_eq!(f.maker_token_balance(&f.taker), 500);
    assert_eq!(f.taker_token_balance(&f.maker), 1_000);

    let info = f.client.get_rfq_order_info(&order);
    assert_eq!(info.status, OrderStatus::Fillable);
    assert_eq!(info.taker_token_filled_amount, 1_000);

    // Over-fill the rest: clamps to remaining 1000 taker tokens.
    let r2 = f.client.fill_rfq_order(&order, &sig, &f.taker, &5_000);
    assert_eq!(r2.taker_token_filled_amount, 1_000);
    assert_eq!(r2.maker_token_filled_amount, 500);
    assert_eq!(f.maker_token_balance(&f.taker), 1_000);
    assert_eq!(f.taker_token_balance(&f.maker), 2_000);

    assert_eq!(
        f.client.get_rfq_order_info(&order).status,
        OrderStatus::Filled
    );
}

#[test]
fn rfq_rejects_unregistered_signer() {
    let f = setup();
    let order = f.rfq_order();
    let hash = f.client.get_rfq_order_hash(&order);
    // Sign with a valid key that is NOT registered to the maker.
    let rogue = SigningKey::from_bytes(&[9u8; 32]);
    let sig = sign(&f.env, &rogue, &hash);

    let err = f
        .client
        .try_fill_rfq_order(&order, &sig, &f.taker, &1_000)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::SignerNotAuthorized.into());
}

#[test]
fn rfq_rejects_wrong_origin() {
    let f = setup();
    let mut order = f.rfq_order();
    // Restrict submission to a different origin.
    order.tx_origin = Address::generate(&f.env);
    let hash = f.client.get_rfq_order_hash(&order);
    let sig = sign(&f.env, &f.maker_key, &hash);

    let err = f
        .client
        .try_fill_rfq_order(&order, &sig, &f.taker, &1_000)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::OrderNotFillableByOrigin.into());

    // Whitelisting the taker as an allowed origin makes it fillable.
    f.client
        .register_allowed_rfq_origin(&order.tx_origin, &f.taker, &true);
    let r = f.client.fill_rfq_order(&order, &sig, &f.taker, &1_000);
    assert_eq!(r.taker_token_filled_amount, 1_000);
}

#[test]
fn rfq_expired() {
    let f = setup();
    let order = f.rfq_order();
    let hash = f.client.get_rfq_order_hash(&order);
    let sig = sign(&f.env, &f.maker_key, &hash);

    f.env.ledger().set_timestamp(order.expiry + 1);

    assert_eq!(
        f.client.get_rfq_order_info(&order).status,
        OrderStatus::Expired
    );
    let err = f
        .client
        .try_fill_rfq_order(&order, &sig, &f.taker, &1_000)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::OrderNotFillable.into());
}

#[test]
fn rfq_cancel() {
    let f = setup();
    let order = f.rfq_order();
    let hash = f.client.get_rfq_order_hash(&order);
    let sig = sign(&f.env, &f.maker_key, &hash);

    f.client.cancel_rfq_order(&order);
    assert_eq!(
        f.client.get_rfq_order_info(&order).status,
        OrderStatus::Cancelled
    );
    let err = f
        .client
        .try_fill_rfq_order(&order, &sig, &f.taker, &1_000)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::OrderNotFillable.into());
}

#[test]
fn cancel_pair_by_salt() {
    let f = setup();
    let order = f.rfq_order(); // salt = 1
    let hash = f.client.get_rfq_order_hash(&order);
    let sig = sign(&f.env, &f.maker_key, &hash);

    // Invalidate everything with salt < 2 for this RFQ pair.
    f.client.cancel_pair_rfq_orders(&f.maker, &f.maker_token, &f.taker_token, &2u64);
    let err = f
        .client
        .try_fill_rfq_order(&order, &sig, &f.taker, &1_000)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::OrderNotFillable.into());
}

#[test]
fn fill_or_kill_reverts_on_partial() {
    let f = setup();
    let order = f.rfq_order();
    let hash = f.client.get_rfq_order_hash(&order);
    let sig = sign(&f.env, &f.maker_key, &hash);

    // Only 2000 taker tokens are available but we demand 3000 exactly.
    let err = f
        .client
        .try_fill_or_kill_rfq_order(&order, &sig, &f.taker, &3_000)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::FillOrKillFailed.into());

    // Exact fill of the full amount succeeds.
    let r = f.client.fill_or_kill_rfq_order(&order, &sig, &f.taker, &2_000);
    assert_eq!(r.maker_token_filled_amount, 1_000);
}

#[test]
fn limit_order_fee_taken_from_maker_output() {
    let f = setup();
    let fee_recipient = Address::generate(&f.env);
    let mut order = f.limit_order();
    order.token_fee_amount = 10; // 10 maker tokens at full fill
    order.fee_recipient = fee_recipient.clone();

    let hash = f.client.get_limit_order_hash(&order);
    let sig = sign(&f.env, &f.maker_key, &hash);

    // Fill half: maker_filled = 500, fee = floor(500 * 10 / 1000) = 5 (maker
    // token), taker receives 495, fee recipient receives 5.
    let r = f.client.fill_limit_order(&order, &sig, &f.taker, &1_000);
    assert_eq!(r.taker_token_filled_amount, 1_000);
    assert_eq!(r.maker_token_filled_amount, 500); // gross
    assert_eq!(r.fee_filled_amount, 5);
    // Fee is paid in the MAKER token, skimmed from the maker output.
    assert_eq!(f.maker_token_balance(&fee_recipient), 5);
    assert_eq!(f.maker_token_balance(&f.taker), 495);
    // Maker received the full taker amount for the fill.
    assert_eq!(f.taker_token_balance(&f.maker), 1_000);
}

#[test]
fn rfq_rejects_wrong_taker() {
    let f = setup();
    let mut order = f.rfq_order();
    let designated = Address::generate(&f.env);
    order.taker = Some(designated);
    let hash = f.client.get_rfq_order_hash(&order);
    let sig = sign(&f.env, &f.maker_key, &hash);

    let err = f
        .client
        .try_fill_rfq_order(&order, &sig, &f.taker, &1_000)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::OrderNotFillableByTaker.into());
}

#[test]
fn hash_is_deterministic_and_pubkey_registered() {
    let f = setup();
    let order = f.rfq_order();
    let h1 = f.client.get_rfq_order_hash(&order);
    let h2 = f.client.get_rfq_order_hash(&order);
    assert_eq!(h1, h2);
    assert!(f.client.is_order_signer(&f.maker, &f.maker_pubkey));
    // Sanity: contract id is a real address (keeps `contract_id` field used).
    assert_ne!(f.contract_id, f.maker);
}
