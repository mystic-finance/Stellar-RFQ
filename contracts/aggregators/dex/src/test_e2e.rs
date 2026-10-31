//! The RFQ router's `Leg::Dex` path with the real router: the router pushes
//! the input, the aggregator executes the encoded route, and neither keeps
//! anything beyond the fees it owes.

use crate::test::{setup, Fixture, BIG};
use crate::*;
use router::{AggregatorLeg, Leg, RouterContractClient, SourceKind};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token::TokenClient;
use soroban_sdk::vec;
use soroban_sdk::xdr::ToXdr;

fn world() -> (Fixture, RouterContractClient<'static>, Address, Address) {
    let f = setup();
    let admin = Address::generate(&f.env);
    let settlement = f.env.register(rfq::RfqContract, ());
    rfq::RfqContractClient::new(&f.env, &settlement).initialize(&admin, &f.c);
    let router_id = f.env.register(router::RouterContract, ());
    let router = RouterContractClient::new(&f.env, &router_id);
    router.initialize(&admin, &settlement);
    router.register_source(&f.agg.address, &SourceKind::Dex, &true);
    f.agg.set_router(&router_id);

    let direct = f.venue(9_900, 0, &[&f.c]);
    let via_b = f.venue(10_000, 0, &[&f.b, &f.c]);
    TokenClient::new(&f.env, &f.a).approve(
        &f.user,
        &router_id,
        &BIG,
        &(f.env.ledger().sequence() + 1_000),
    );
    (f, router, direct, via_b)
}

impl Fixture {
    fn dex_leg(&self, amount: i128, hops: Vec<Hop>) -> Leg {
        let route = vec![
            &self.env,
            Split {
                amount_in: amount,
                hops,
            },
        ];
        Leg::Dex(AggregatorLeg {
            aggregator: self.agg.address.clone(),
            taker_amount: amount,
            min_maker_amount: 1,
            data: Route::Splits(route).to_xdr(&self.env),
        })
    }
}

#[test]
fn dex_legs_settle_through_the_router_and_leave_only_owed_fees() {
    let (f, router, direct, via_b) = world();
    let legs = vec![
        &f.env,
        f.dex_leg(600_000, vec![&f.env, f.hop(&direct, &f.c)]),
        f.dex_leg(
            400_000,
            vec![&f.env, f.hop(&via_b, &f.b), f.hop(&via_b, &f.c)],
        ),
    ];
    // 594,000 + 400,000 gross, less 10 bps on each leg.
    let r = router.fill(&f.user, &f.a, &f.c, &legs, &993_006);
    assert_eq!(r.taker_spent, 1_000_000);
    assert_eq!(r.amount_out, 993_006);
    assert_eq!(f.bal(&f.c, &f.user), 993_006);

    for token in [&f.a, &f.b, &f.c] {
        assert_eq!(f.bal(token, &router.address), 0);
        assert_eq!(f.bal(token, &f.agg.address), f.agg.total_owed(token));
    }
    assert_eq!(f.agg.owed(&f.protocol, &f.c), 994);
}

#[test]
fn the_route_reverts_whole_when_the_seller_would_receive_too_little() {
    let (f, router, direct, _) = world();
    let legs = vec![
        &f.env,
        f.dex_leg(1_000_000, vec![&f.env, f.hop(&direct, &f.c)]),
    ];
    assert!(router
        .try_fill(&f.user, &f.a, &f.c, &legs, &989_011)
        .is_err());
    assert_eq!(f.bal(&f.a, &f.user), BIG);
    assert_eq!(f.bal(&f.c, &f.user), 0);
}
