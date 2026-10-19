//! Calldata routes, under real auth: only the user's signature is mocked, so a
//! router's pull of the input succeeds only through the route's own
//! pre-authorisation, which the aggregator restricts to `token_in`.

use crate::test::{setup, Fixture, BIG};
use crate::*;
use soroban_sdk::auth::{ContractContext, SubContractInvocation};
use soroban_sdk::testutils::{Address as _, MockAuth, MockAuthInvoke};
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{contract, contractimpl, vec, IntoVal};

/// A third-party router: pulls `amount` of `token_in` from `from` (the
/// aggregator) and pays `rate_bps` of it in `token_out` back to `from`.
#[contract]
pub struct MockRouter;

#[contractimpl]
impl MockRouter {
    pub fn swap(
        env: Env,
        from: Address,
        token_in: Address,
        token_out: Address,
        amount: i128,
        rate_bps: i128,
    ) {
        let me = env.current_contract_address();
        token::Client::new(&env, &token_in).transfer(&from, &me, &amount);
        token::Client::new(&env, &token_out).transfer(&me, &from, &(amount * rate_bps / 10_000));
    }
}

struct W {
    f: Fixture,
    router: Address,
}

fn world() -> W {
    let f = setup();
    let router = f.env.register(MockRouter, ());
    StellarAssetClient::new(&f.env, &f.c).mint(&router, &BIG);
    StellarAssetClient::new(&f.env, &f.a).mint(&router, &BIG);
    f.agg.allow_target(&router, &true);
    W { f, router }
}

impl W {
    fn transfer_auth(&self, token: &Address, amount: i128) -> InvokerContractAuthEntry {
        let e = &self.f.env;
        InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: token.clone(),
                fn_name: Symbol::new(e, "transfer"),
                args: (self.f.agg.address.clone(), self.router.clone(), amount).into_val(e),
            },
            sub_invocations: Vec::new(e),
        })
    }

    fn call(&self, pull: i128, auth: Vec<InvokerContractAuthEntry>) -> Call {
        let e = &self.f.env;
        Call {
            target: self.router.clone(),
            function: Symbol::new(e, "swap"),
            args: (
                self.f.agg.address.clone(),
                self.f.a.clone(),
                self.f.c.clone(),
                pull,
                9_900i128,
            )
                .into_val(e),
            auth,
        }
    }

    /// Only the user's own signature (the call and its input transfer) is mocked.
    fn swap(&self, amount: i128, min: i128, calls: Vec<Call>) -> Result<SwapResult, ()> {
        let f = &self.f;
        let desc = f.desc(amount, min);
        f.env.mock_auths(&[MockAuth {
            address: &f.user,
            invoke: &MockAuthInvoke {
                contract: &f.agg.address,
                fn_name: "swap_calls",
                args: (
                    f.user.clone(),
                    desc.clone(),
                    calls.clone(),
                    Option::<Partner>::None,
                )
                    .into_val(&f.env),
                sub_invokes: &[MockAuthInvoke {
                    contract: &f.a,
                    fn_name: "transfer",
                    args: (f.user.clone(), f.agg.address.clone(), amount).into_val(&f.env),
                    sub_invokes: &[],
                }],
            },
        }]);
        let r = f.agg.try_swap_calls(&f.user, &desc, &calls, &None);
        f.env.mock_all_auths();
        match r {
            Ok(Ok(v)) => Ok(v),
            _ => Err(()),
        }
    }

    fn bal(&self, t: &Address, who: &Address) -> i128 {
        TokenClient::new(&self.f.env, t).balance(who)
    }
}

#[test]
fn calldata_executes_takes_the_fee_and_refunds_unspent_input() {
    let w = world();
    let (a, c) = (w.f.a.clone(), w.f.c.clone());
    // The route spends 800k of the 1M in; 200k comes back.
    let calls = vec![
        &w.f.env,
        w.call(800_000, vec![&w.f.env, w.transfer_auth(&a, 800_000)]),
    ];
    let r = w.swap(1_000_000, 790_000, calls).unwrap();
    assert_eq!(r.gross_out, 792_000);
    assert_eq!(r.protocol_fee, 792);
    assert_eq!(w.bal(&c, &w.f.user), 791_208);
    assert_eq!(w.bal(&a, &w.f.user), BIG - 800_000);
    assert_eq!(w.bal(&a, &w.f.agg.address), 0);
    assert_eq!(w.bal(&c, &w.f.agg.address), w.f.agg.total_owed(&c));
}

#[test]
fn a_route_cannot_authorise_another_token_or_more_than_amount_in() {
    let w = world();
    let e = &w.f.env;
    let other = vec![e, w.call(1_000, vec![e, w.transfer_auth(&w.f.c, 1_000)])];
    assert!(w.swap(1_000, 1, other).is_err());
    let greedy = vec![e, w.call(1_001, vec![e, w.transfer_auth(&w.f.a, 1_001)])];
    assert!(w.swap(1_000, 1, greedy).is_err());
    // Without the entry, the router cannot pull at all.
    assert!(w
        .swap(1_000, 1, vec![e, w.call(1_000, Vec::new(e))])
        .is_err());
}

#[test]
fn only_allowlisted_routers_are_called_and_never_a_token() {
    let w = world();
    let e = &w.f.env;
    let mut stray = w.call(1_000, vec![e, w.transfer_auth(&w.f.a, 1_000)]);
    stray.target = e.register(MockRouter, ());
    assert!(w.swap(1_000, 1, vec![e, stray]).is_err());

    w.f.agg.allow_target(&w.f.a, &true);
    let drain = Call {
        target: w.f.a.clone(),
        function: Symbol::new(e, "transfer"),
        args: (w.f.agg.address.clone(), w.f.user.clone(), 1i128).into_val(e),
        auth: Vec::new(e),
    };
    assert!(w.swap(1_000, 1, vec![e, drain]).is_err());
}

#[test]
fn the_rfq_router_can_fill_with_calldata() {
    let w = world();
    let f = &w.f;
    let calls = vec![
        &f.env,
        w.call(500, vec![&f.env, w.transfer_auth(&f.a, 500)]),
    ];
    let router = Address::generate(&f.env);
    f.agg.set_router(&router);
    StellarAssetClient::new(&f.env, &f.a).mint(&f.agg.address, &500);
    let out = f.agg.fill(
        &router,
        &f.a,
        &f.c,
        &500,
        &1,
        &Route::Calls(calls).to_xdr(&f.env),
    );
    assert_eq!(out, 495);
    assert_eq!(w.bal(&f.c, &router), 495);
}
