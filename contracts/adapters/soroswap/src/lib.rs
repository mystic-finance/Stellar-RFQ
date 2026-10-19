#![no_std]
//! DexAdapter over the Soroswap router. One shared instance; callable only by
//! the DEX aggregator. Single-hop per call — multi-hop routes are hops in the
//! aggregator's route, so each hop's output is measured there.

use common::interfaces::DexAdapter;
use soroban_sdk::auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation};
use soroban_sdk::{
    contract, contractclient, contractimpl, contracttype, token, vec, Address, Bytes, Env, IntoVal,
    Symbol, Vec,
};

#[cfg(test)]
mod test;

/// The Soroswap router functions this adapter uses, as deployed.
#[contractclient(name = "SoroswapRouterClient")]
pub trait SoroswapRouter {
    fn swap_exact_tokens_for_tokens(
        e: Env,
        amount_in: i128,
        amount_out_min: i128,
        path: Vec<Address>,
        to: Address,
        deadline: u64,
    ) -> Vec<i128>;
    fn router_get_amounts_out(e: Env, amount_in: i128, path: Vec<Address>) -> Vec<i128>;
    fn router_pair_for(e: Env, token_a: Address, token_b: Address) -> Address;
}

#[contracttype]
enum Key {
    Aggregator,
    Router,
}

#[contract]
pub struct SoroswapAdapter;

#[contractimpl]
impl SoroswapAdapter {
    pub fn __constructor(env: Env, aggregator: Address, router: Address) {
        env.storage().instance().set(&Key::Aggregator, &aggregator);
        env.storage().instance().set(&Key::Router, &router);
    }

    fn router(env: &Env) -> SoroswapRouterClient<'_> {
        let router: Address = env.storage().instance().get(&Key::Router).unwrap();
        SoroswapRouterClient::new(env, &router)
    }
}

#[contractimpl]
impl DexAdapter for SoroswapAdapter {
    fn quote(
        env: Env,
        token_in: Address,
        token_out: Address,
        amount_in: i128,
        _data: Bytes,
    ) -> i128 {
        let amounts =
            Self::router(&env).router_get_amounts_out(&amount_in, &vec![&env, token_in, token_out]);
        amounts.last().unwrap_or(0)
    }

    fn swap(
        env: Env,
        token_in: Address,
        token_out: Address,
        amount_in: i128,
        min_out: i128,
        _data: Bytes,
        to: Address,
    ) -> i128 {
        let aggregator: Address = env.storage().instance().get(&Key::Aggregator).unwrap();
        aggregator.require_auth();
        let me = env.current_contract_address();
        let router = Self::router(&env);

        // The router pulls the input from `to` (us) inside its own call, one
        // level below this one, so that exact transfer is authorised up front.
        // The entry covers only the next call this contract makes.
        let pair = router.router_pair_for(&token_in, &token_out);
        let out = token::Client::new(&env, &token_out);
        let before = out.balance(&me);
        env.authorize_as_current_contract(vec![
            &env,
            InvokerContractAuthEntry::Contract(SubContractInvocation {
                context: ContractContext {
                    contract: token_in.clone(),
                    fn_name: Symbol::new(&env, "transfer"),
                    args: (me.clone(), pair, amount_in).into_val(&env),
                },
                sub_invocations: Vec::new(&env),
            }),
        ]);
        router.swap_exact_tokens_for_tokens(
            &amount_in,
            &min_out,
            &vec![&env, token_in, token_out.clone()],
            &me,
            &(env.ledger().timestamp() + 1),
        );
        let received = out.balance(&me) - before;
        out.transfer(&me, &to, &received);
        received
    }
}
