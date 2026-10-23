use crate::types::PriceData;
use soroban_sdk::{contractclient, Address, Bytes, BytesN, Env, Symbol, Val, Vec};

/// What a vault needs from the facility aggregator that deployed it. Only a
/// vault the aggregator deployed may call `deploy_adapter`.
#[contractclient(name = "VaultHostClient")]
pub trait VaultHost {
    fn admin(env: Env) -> Address;
    /// The code a vault may upgrade to.
    fn vault_wasm(env: Env) -> BytesN<32>;
    /// Deploys an allowlisted venue adapter owned by `vault`.
    fn deploy_adapter(
        env: Env,
        vault: Address,
        wasm_hash: BytesN<32>,
        asset: Address,
        venue: Address,
    ) -> Address;
}

/// A source the RFQ router can route a leg through: the facility aggregator or
/// the DEX aggregator. For a DEX leg the router pushes `amount_in` to the
/// source before `fill`; for a Facility leg it moves nothing and the facility
/// pulls from the payer named in `data`. No source holds an allowance.
#[contractclient(name = "AggregatorClient")]
pub trait Aggregator {
    fn quote(env: Env, token_in: Address, token_out: Address, amount_in: i128) -> i128;

    fn fill(
        env: Env,
        recipient: Address,
        token_in: Address,
        token_out: Address,
        amount_in: i128,
        min_amount_out: i128,
        data: Bytes,
    ) -> i128;
}

/// A yield venue, as one vault sees it. Each instance is owned by exactly one
/// vault and serves no one else. `total_assets` is what the position is worth;
/// `max_withdraw` is what can be pulled inside the current transaction.
///
/// Every adapter is deployed by the facility aggregator with the same
/// constructor, `__constructor(owner, asset, venue)`: the owning vault, its base
/// asset, and the one external venue the adapter targets (a Blend pool, a
/// DeFindex vault, a SEP-56 vault).
#[contractclient(name = "VenueAdapterClient")]
pub trait VenueAdapter {
    /// The vault has already transferred `amount` of its base asset; supply it.
    fn deposit(env: Env, amount: i128) -> i128;
    /// Withdraw up to `amount` to `to`; returns what was actually sent.
    fn withdraw(env: Env, amount: i128, to: Address) -> i128;
    fn total_assets(env: Env) -> i128;
    fn max_withdraw(env: Env) -> i128;
}

/// A swap venue, as the DEX aggregator sees it. Stateless and shared; holds
/// nothing between transactions.
#[contractclient(name = "DexAdapterClient")]
pub trait DexAdapter {
    fn quote(env: Env, token_in: Address, token_out: Address, amount_in: i128, data: Bytes)
        -> i128;

    /// The aggregator has already transferred `amount_in`; swap and send to `to`.
    fn swap(
        env: Env,
        token_in: Address,
        token_out: Address,
        amount_in: i128,
        min_out: i128,
        data: Bytes,
        to: Address,
    ) -> i128;
}

/// A price feed: our `oracle` adapter, or anything with the same view.
#[contractclient(name = "OracleClient")]
pub trait Oracle {
    fn get_price(env: Env, base: Address, quote: Address) -> PriceData;
}

/// Who may hold a claim token on `rwa`: typically the RWA's own holder allowlist.
#[contractclient(name = "GateClient")]
pub trait Gate {
    fn can_hold(env: Env, rwa: Address, account: Address) -> bool;
}

/// An allowlist of calls `caller` may have its `account` make.
#[contractclient(name = "CallVerifierClient")]
pub trait CallVerifier {
    fn verify(
        env: Env,
        caller: Address,
        account: Address,
        target: Address,
        func: Symbol,
        args: Vec<Val>,
    ) -> bool;
}

/// A redemption vault's issuer-facing account. Only its vault may call it.
#[contractclient(name = "AccountClient")]
pub trait RedemptionAccount {
    fn execute(env: Env, target: Address, func: Symbol, args: Vec<Val>) -> Val;
    fn approve(env: Env, token: Address, spender: Address, amount: i128);
    fn sweep(env: Env, token: Address, to: Address) -> i128;
}

/// A redemption batch's claim token, beyond SEP-41: only its vault mints and burns.
#[contractclient(name = "ClaimTokenClient")]
pub trait ClaimToken {
    fn mint(env: Env, to: Address, amount: i128);
    fn burn_claim(env: Env, from: Address, amount: i128);
    fn set_gate(env: Env, gate: Option<Address>);
    fn total_supply(env: Env) -> i128;
}
