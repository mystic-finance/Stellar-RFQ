#![no_std]
//! # Stellar RFQ — 0x-style on-chain settlement for Soroban
//!
//! A faithful Soroban port of 0x's `NativeOrdersSettlement` mixin. Makers sign
//! **RFQ** or **limit** orders off-chain (ed25519 over the canonical order hash);
//! takers submit them on-chain and the contract settles the trade atomically,
//! moving the maker and taker tokens via the Soroban token interface.
//!
//! ## How authorisation maps from EVM to Soroban
//!
//! | 0x concept | Soroban equivalent |
//! |---|---|
//! | Maker `approve`s the Exchange Proxy | Maker `approve`s an allowance to this contract for the maker token |
//! | Taker `approve`s the Exchange Proxy | Taker `approve`s an allowance to this contract for the taker token |
//! | EIP-712 signed order | ed25519 signature over [`hash::rfq_order_hash`] / [`hash::limit_order_hash`] |
//! | `registerAllowedOrderSigner` | [`RfqContract::register_order_signer`] |
//! | `registerAllowedRfqOrigins` | [`RfqContract::register_allowed_rfq_origin`] |
//! | `tx.origin` gate | the on-chain submitter (`taker`) is checked against the order's `tx_origin` |
//! | Settlement `transferFrom` | `token::Client::transfer_from` using this contract as spender |
//!
//! Because the maker is offline, per-order authorisation comes from the ed25519
//! signature (bound to the maker's registered key), while custody of funds comes
//! from the pre-existing token allowance — exactly 0x's model.

mod errors;
mod hash;
mod storage;
mod types;

#[cfg(test)]
mod test;

use soroban_sdk::{
    contract, contractimpl, panic_with_error, symbol_short, token, xdr::ToXdr, Address, Bytes,
    BytesN, Env, U256,
};

pub use errors::Error;
pub use types::{FillResult, LimitOrder, OrderInfo, OrderStatus, RfqOrder, Signature};

use storage::OrderKind;

#[contract]
pub struct RfqContract;

#[contractimpl]
impl RfqContract {
    // ---------------------------------------------------------------------
    // Lifecycle / admin
    // ---------------------------------------------------------------------

    /// Initialise the contract with an `admin` (who may set the protocol fee).
    pub fn initialize(env: Env, admin: Address) {
        if storage::has_admin(&env) {
            panic_with_error!(&env, Error::AlreadyInitialized);
        }
        storage::set_admin(&env, &admin);
        storage::extend_instance_ttl(&env);
        env.events()
            .publish((symbol_short!("init"),), admin);
    }

    pub fn get_admin(env: Env) -> Address {
        storage::get_admin(&env)
    }

    /// Upgrade the contract's WASM. Admin only. (Soroban's standard upgrade
    /// path; the settlement logic itself takes no protocol fee.)
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        if !storage::has_admin(&env) {
            panic_with_error!(&env, Error::NotInitialized);
        }
        storage::get_admin(&env).require_auth();
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    // ---------------------------------------------------------------------
    // Registries (signers & origins)
    // ---------------------------------------------------------------------

    /// Register (or revoke) an ed25519 public key allowed to sign orders on
    /// behalf of `maker`. A maker must register their own signing key here
    /// before any order they sign can be filled. Equivalent to 0x
    /// `registerAllowedOrderSigner`.
    pub fn register_order_signer(env: Env, maker: Address, signer: BytesN<32>, allowed: bool) {
        maker.require_auth();
        storage::set_order_signer(&env, &maker, &signer, allowed);
        env.events().publish(
            (symbol_short!("signer"), maker),
            (signer, allowed),
        );
    }

    pub fn is_order_signer(env: Env, maker: Address, signer: BytesN<32>) -> bool {
        storage::is_order_signer(&env, &maker, &signer)
    }

    /// Whitelist (or revoke) a `submitter` allowed to fill RFQ orders whose
    /// `tx_origin` is `origin_owner`. Equivalent to 0x `registerAllowedRfqOrigins`.
    pub fn register_allowed_rfq_origin(
        env: Env,
        origin_owner: Address,
        submitter: Address,
        allowed: bool,
    ) {
        origin_owner.require_auth();
        storage::set_allowed_origin(&env, &origin_owner, &submitter, allowed);
        env.events().publish(
            (symbol_short!("origin"), origin_owner),
            (submitter, allowed),
        );
    }

    // ---------------------------------------------------------------------
    // Views: hashes & info
    // ---------------------------------------------------------------------

    pub fn get_rfq_order_hash(env: Env, order: RfqOrder) -> BytesN<32> {
        hash::rfq_order_hash(&env, &order)
    }

    pub fn get_limit_order_hash(env: Env, order: LimitOrder) -> BytesN<32> {
        hash::limit_order_hash(&env, &order)
    }

    pub fn get_rfq_order_info(env: Env, order: RfqOrder) -> OrderInfo {
        let order_hash = hash::rfq_order_hash(&env, &order);
        let status = Self::status_of(
            &env,
            OrderKind::Rfq,
            &order.maker,
            &order.maker_token,
            &order.taker_token,
            order.maker_amount,
            order.taker_amount,
            order.expiry,
            order.salt,
            &order_hash,
        );
        OrderInfo {
            order_hash: order_hash.clone(),
            status,
            taker_token_filled_amount: storage::get_filled(&env, &order_hash),
        }
    }

    pub fn get_limit_order_info(env: Env, order: LimitOrder) -> OrderInfo {
        let order_hash = hash::limit_order_hash(&env, &order);
        let status = Self::status_of(
            &env,
            OrderKind::Limit,
            &order.maker,
            &order.maker_token,
            &order.taker_token,
            order.maker_amount,
            order.taker_amount,
            order.expiry,
            order.salt,
            &order_hash,
        );
        OrderInfo {
            order_hash: order_hash.clone(),
            status,
            taker_token_filled_amount: storage::get_filled(&env, &order_hash),
        }
    }

    // ---------------------------------------------------------------------
    // RFQ fills
    // ---------------------------------------------------------------------

    /// Fill an RFQ order for up to `taker_token_fill_amount` taker tokens.
    /// The taker is the caller. Returns the realised fill amounts.
    pub fn fill_rfq_order(
        env: Env,
        order: RfqOrder,
        signature: Signature,
        taker: Address,
        taker_token_fill_amount: i128,
    ) -> FillResult {
        taker.require_auth();
        Self::fill_rfq(&env, &order, &signature, &taker, taker_token_fill_amount, false)
    }

    /// Fill an RFQ order for *exactly* `taker_token_fill_amount` taker tokens or
    /// revert (fill-or-kill).
    pub fn fill_or_kill_rfq_order(
        env: Env,
        order: RfqOrder,
        signature: Signature,
        taker: Address,
        taker_token_fill_amount: i128,
    ) -> FillResult {
        taker.require_auth();
        Self::fill_rfq(&env, &order, &signature, &taker, taker_token_fill_amount, true)
    }

    // ---------------------------------------------------------------------
    // Limit fills
    // ---------------------------------------------------------------------

    /// Fill a limit order for up to `taker_token_fill_amount` taker tokens.
    pub fn fill_limit_order(
        env: Env,
        order: LimitOrder,
        signature: Signature,
        taker: Address,
        taker_token_fill_amount: i128,
    ) -> FillResult {
        taker.require_auth();
        Self::fill_limit(&env, &order, &signature, &taker, taker_token_fill_amount, false)
    }

    /// Fill a limit order for *exactly* `taker_token_fill_amount` or revert.
    pub fn fill_or_kill_limit_order(
        env: Env,
        order: LimitOrder,
        signature: Signature,
        taker: Address,
        taker_token_fill_amount: i128,
    ) -> FillResult {
        taker.require_auth();
        Self::fill_limit(&env, &order, &signature, &taker, taker_token_fill_amount, true)
    }

    // ---------------------------------------------------------------------
    // Cancellation
    // ---------------------------------------------------------------------

    /// Cancel a single RFQ order. Maker only.
    pub fn cancel_rfq_order(env: Env, order: RfqOrder) {
        order.maker.require_auth();
        let order_hash = hash::rfq_order_hash(&env, &order);
        storage::set_cancelled(&env, &order_hash);
        env.events()
            .publish((symbol_short!("rfq_cxl"), order.maker), order_hash);
    }

    /// Cancel a single limit order. Maker only.
    pub fn cancel_limit_order(env: Env, order: LimitOrder) {
        order.maker.require_auth();
        let order_hash = hash::limit_order_hash(&env, &order);
        storage::set_cancelled(&env, &order_hash);
        env.events()
            .publish((symbol_short!("lim_cxl"), order.maker), order_hash);
    }

    /// Cancel all of a maker's **RFQ** orders for a `(maker_token, taker_token)`
    /// pair whose `salt < min_salt`. Equivalent to 0x `cancelPairRfqOrders`.
    pub fn cancel_pair_rfq_orders(
        env: Env,
        maker: Address,
        maker_token: Address,
        taker_token: Address,
        min_salt: u64,
    ) {
        maker.require_auth();
        storage::set_min_salt(&env, OrderKind::Rfq, &maker, &maker_token, &taker_token, min_salt);
        env.events().publish(
            (symbol_short!("rfq_pcxl"), maker),
            (maker_token, taker_token, min_salt),
        );
    }

    /// Cancel all of a maker's **limit** orders for a `(maker_token,
    /// taker_token)` pair whose `salt < min_salt`. Equivalent to 0x
    /// `cancelPairLimitOrders`.
    pub fn cancel_pair_limit_orders(
        env: Env,
        maker: Address,
        maker_token: Address,
        taker_token: Address,
        min_salt: u64,
    ) {
        maker.require_auth();
        storage::set_min_salt(&env, OrderKind::Limit, &maker, &maker_token, &taker_token, min_salt);
        env.events().publish(
            (symbol_short!("lim_pcxl"), maker),
            (maker_token, taker_token, min_salt),
        );
    }

    // =====================================================================
    // Internal helpers
    // =====================================================================

    fn fill_rfq(
        env: &Env,
        order: &RfqOrder,
        signature: &Signature,
        taker: &Address,
        taker_token_fill_amount: i128,
        fill_or_kill: bool,
    ) -> FillResult {
        if taker_token_fill_amount <= 0 {
            panic_with_error!(env, Error::InvalidFillAmount);
        }
        let order_hash = hash::rfq_order_hash(env, order);

        // Must be fillable.
        let status = Self::status_of(
            env,
            OrderKind::Rfq,
            &order.maker,
            &order.maker_token,
            &order.taker_token,
            order.maker_amount,
            order.taker_amount,
            order.expiry,
            order.salt,
            &order_hash,
        );
        if status != OrderStatus::Fillable {
            panic_with_error!(env, Error::OrderNotFillable);
        }

        // Must be submitted by an allowed origin (0x `tx.origin` gate).
        if &order.tx_origin != taker && !storage::is_allowed_origin(env, &order.tx_origin, taker) {
            panic_with_error!(env, Error::OrderNotFillableByOrigin);
        }

        // Must be fillable by this taker.
        if let Some(allowed_taker) = &order.taker {
            if allowed_taker != taker {
                panic_with_error!(env, Error::OrderNotFillableByTaker);
            }
        }

        // Signature must be valid and from a registered signer.
        Self::verify_signature(env, &order.maker, &order_hash, signature);

        let already_filled = storage::get_filled(env, &order_hash);
        // RFQ orders carry no fee.
        let (taker_filled, maker_filled, _fee) = Self::settle(
            env,
            &order_hash,
            &order.maker,
            taker, // payer
            taker, // recipient
            &order.maker_token,
            &order.taker_token,
            order.maker_amount,
            order.taker_amount,
            taker_token_fill_amount,
            already_filled,
            0,
            None,
        );

        if fill_or_kill && taker_filled < taker_token_fill_amount {
            panic_with_error!(env, Error::FillOrKillFailed);
        }

        env.events().publish(
            (symbol_short!("rfq_fill"), order_hash, order.maker.clone(), taker.clone()),
            (
                order.maker_token.clone(),
                order.taker_token.clone(),
                taker_filled,
                maker_filled,
                order.pool.clone(),
            ),
        );

        FillResult {
            taker_token_filled_amount: taker_filled,
            maker_token_filled_amount: maker_filled,
            fee_filled_amount: 0,
        }
    }

    fn fill_limit(
        env: &Env,
        order: &LimitOrder,
        signature: &Signature,
        taker: &Address,
        taker_token_fill_amount: i128,
        fill_or_kill: bool,
    ) -> FillResult {
        if taker_token_fill_amount <= 0 {
            panic_with_error!(env, Error::InvalidFillAmount);
        }
        let order_hash = hash::limit_order_hash(env, order);

        let status = Self::status_of(
            env,
            OrderKind::Limit,
            &order.maker,
            &order.maker_token,
            &order.taker_token,
            order.maker_amount,
            order.taker_amount,
            order.expiry,
            order.salt,
            &order_hash,
        );
        if status != OrderStatus::Fillable {
            panic_with_error!(env, Error::OrderNotFillable);
        }

        // Must be fillable by this taker.
        if let Some(allowed_taker) = &order.taker {
            if allowed_taker != taker {
                panic_with_error!(env, Error::OrderNotFillableByTaker);
            }
        }
        // Must be submitted by the allowed sender (caller == taker == sender).
        if let Some(allowed_sender) = &order.sender {
            if allowed_sender != taker {
                panic_with_error!(env, Error::OrderNotFillableBySender);
            }
        }

        Self::verify_signature(env, &order.maker, &order_hash, signature);

        let already_filled = storage::get_filled(env, &order_hash);
        // The fee is skimmed from the maker output inside `settle`.
        let (taker_filled, maker_filled, fee_filled) = Self::settle(
            env,
            &order_hash,
            &order.maker,
            taker,
            taker,
            &order.maker_token,
            &order.taker_token,
            order.maker_amount,
            order.taker_amount,
            taker_token_fill_amount,
            already_filled,
            order.token_fee_amount,
            Some(&order.fee_recipient),
        );

        if fill_or_kill && taker_filled < taker_token_fill_amount {
            panic_with_error!(env, Error::FillOrKillFailed);
        }

        env.events().publish(
            (symbol_short!("lim_fill"), order_hash, order.maker.clone(), taker.clone()),
            (
                order.maker_token.clone(),
                order.taker_token.clone(),
                order.fee_recipient.clone(),
                taker_filled,
                maker_filled,
                fee_filled,
                order.pool.clone(),
            ),
        );

        FillResult {
            taker_token_filled_amount: taker_filled,
            maker_token_filled_amount: maker_filled,
            fee_filled_amount: fee_filled,
        }
    }

    /// Settle the trade between maker and taker. Returns realised
    /// `(taker_filled, maker_filled, fee_filled)`. Equivalent to 0x
    /// `_settleOrder`, but the fee (if any) is skimmed from the **maker token**
    /// output and routed to `fee_recipient`, so the taker receives
    /// `maker_filled - fee_filled`.
    #[allow(clippy::too_many_arguments)]
    fn settle(
        env: &Env,
        order_hash: &BytesN<32>,
        maker: &Address,
        payer: &Address,
        recipient: &Address,
        maker_token: &Address,
        taker_token: &Address,
        maker_amount: i128,
        taker_amount: i128,
        taker_token_fill_amount: i128,
        already_filled: i128,
        token_fee_amount: i128,
        fee_recipient: Option<&Address>,
    ) -> (i128, i128, i128) {
        // Clamp the taker fill to what remains fillable.
        let remaining = taker_amount - already_filled;
        let taker_filled = if taker_token_fill_amount < remaining {
            taker_token_fill_amount
        } else {
            remaining
        };
        if taker_filled <= 0 {
            return (0, 0, 0);
        }
        // maker_filled = floor(taker_filled * maker_amount / taker_amount)
        let maker_filled = Self::mul_div_floor(env, taker_filled, maker_amount, taker_amount);
        if maker_filled <= 0 {
            return (0, 0, 0);
        }

        // Record the fill before moving funds.
        storage::set_filled(env, order_hash, already_filled + taker_filled);

        let contract = env.current_contract_address();
        // Taker token: payer -> maker.
        token::Client::new(env, taker_token)
            .transfer_from(&contract, payer, maker, &taker_filled);

        // Fee comes out of the maker output (in maker token), proportional to
        // fill: floor(maker_filled * token_fee_amount / maker_amount).
        let mut fee_filled = 0i128;
        if token_fee_amount > 0 {
            if let Some(_fr) = fee_recipient {
                fee_filled = Self::mul_div_floor(env, maker_filled, token_fee_amount, maker_amount);
                if fee_filled > maker_filled {
                    fee_filled = maker_filled;
                }
            }
        }
        let to_recipient = maker_filled - fee_filled;

        let maker_client = token::Client::new(env, maker_token);
        // Maker token: maker -> taker (net of fee).
        if to_recipient > 0 {
            maker_client.transfer_from(&contract, maker, recipient, &to_recipient);
        }
        // Maker token: maker -> fee recipient.
        if fee_filled > 0 {
            maker_client.transfer_from(&contract, maker, fee_recipient.unwrap(), &fee_filled);
        }

        (taker_filled, maker_filled, fee_filled)
    }

    fn verify_signature(
        env: &Env,
        maker: &Address,
        order_hash: &BytesN<32>,
        signature: &Signature,
    ) {
        // The maker signs the SEP-53 digest of the order hash (a wallet's
        // `signMessage` or a bot produce the same), so verify over that digest.
        // Traps if the signature does not verify.
        let digest = hash::sep53_digest(env, order_hash);
        let message = Bytes::from_array(env, &digest.to_array());
        env.crypto()
            .ed25519_verify(&signature.signer, &message, &signature.signature);
        // Like 0x's `signer == maker || isValidOrderSigner`: accept the maker's
        // own account key with NO registration, or a key the maker registered as
        // a delegated signer.
        let signed_by_maker = match Self::maker_account_pubkey(env, maker) {
            Some(pk) => pk == signature.signer,
            None => false,
        };
        if !signed_by_maker && !storage::is_order_signer(env, maker, &signature.signer) {
            panic_with_error!(env, Error::SignerNotAuthorized);
        }
    }

    /// Recover the ed25519 public key of a maker **account** address (`G...`).
    /// Returns `None` for contract addresses. Relies on the canonical ScVal XDR
    /// layout of an account address:
    /// `[SCV_ADDRESS(4)][SC_ADDRESS_TYPE_ACCOUNT(4)][PUBLIC_KEY_TYPE_ED25519(4)][pubkey(32)]`.
    fn maker_account_pubkey(env: &Env, maker: &Address) -> Option<BytesN<32>> {
        let xdr = maker.clone().to_xdr(env);
        if xdr.len() != 44 {
            return None;
        }
        let mut pk = [0u8; 32];
        let mut i = 0u32;
        while i < 32 {
            pk[i as usize] = xdr.get(12 + i).unwrap();
            i += 1;
        }
        Some(BytesN::from_array(env, &pk))
    }

    #[allow(clippy::too_many_arguments)]
    fn status_of(
        env: &Env,
        kind: OrderKind,
        maker: &Address,
        maker_token: &Address,
        taker_token: &Address,
        maker_amount: i128,
        taker_amount: i128,
        expiry: u64,
        salt: u64,
        order_hash: &BytesN<32>,
    ) -> OrderStatus {
        if maker_amount <= 0 || taker_amount <= 0 {
            return OrderStatus::Invalid;
        }
        if storage::is_cancelled(env, order_hash) {
            return OrderStatus::Cancelled;
        }
        if salt < storage::get_min_salt(env, kind, maker, maker_token, taker_token) {
            return OrderStatus::Cancelled;
        }
        if env.ledger().timestamp() >= expiry {
            return OrderStatus::Expired;
        }
        if storage::get_filled(env, order_hash) >= taker_amount {
            return OrderStatus::Filled;
        }
        OrderStatus::Fillable
    }

    /// `floor(a * b / denom)` computed in 256-bit to avoid `i128` overflow.
    /// All inputs are expected to be non-negative.
    fn mul_div_floor(env: &Env, a: i128, b: i128, denom: i128) -> i128 {
        let a = U256::from_u128(env, a as u128);
        let b = U256::from_u128(env, b as u128);
        let d = U256::from_u128(env, denom as u128);
        match a.mul(&b).div(&d).to_u128() {
            Some(v) => v as i128,
            None => panic_with_error!(env, Error::Overflow),
        }
    }
}
