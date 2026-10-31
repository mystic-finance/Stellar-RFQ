use super::*;
use mock_dex::MockDex;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{vec, String};

pub(crate) const BIG: i128 = 1_000_000_000_000;

pub(crate) struct Fixture {
    pub env: Env,
    pub agg: DexAggregatorClient<'static>,
    pub protocol: Address,
    pub user: Address,
    pub a: Address,
    pub b: Address,
    pub c: Address,
}

pub(crate) fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    env.cost_estimate().budget().reset_unlimited();
    let admin = Address::generate(&env);
    let protocol = Address::generate(&env);
    let user = Address::generate(&env);
    let issuer = Address::generate(&env);
    let [a, b, c] = [0, 1, 2].map(|_| {
        env.register_stellar_asset_contract_v2(issuer.clone())
            .address()
    });
    let agg_id = env.register(
        DexAggregator,
        (
            admin,
            Address::generate(&env),
            FeeConfig {
                recipient: protocol.clone(),
                protocol_fee_bps: 10,
                max_partner_fee_bps: 100,
                partner_share_bps: 2_000,
            },
        ),
    );
    StellarAssetClient::new(&env, &a).mint(&user, &BIG);
    Fixture {
        agg: DexAggregatorClient::new(&env, &agg_id),
        env,
        protocol,
        user,
        a,
        b,
        c,
    }
}

impl Fixture {
    /// An allowlisted venue paying `rate_bps` of input, stocked with `stock`.
    pub(crate) fn venue(&self, rate_bps: i128, shortfall: i128, stock: &[&Address]) -> Address {
        let v = self.env.register(MockDex, (rate_bps, shortfall));
        for t in stock {
            StellarAssetClient::new(&self.env, t).mint(&v, &BIG);
        }
        self.agg.allow_venue(&v, &true);
        v
    }

    pub(crate) fn hop(&self, venue: &Address, token_out: &Address) -> Hop {
        Hop {
            venue: venue.clone(),
            token_out: token_out.clone(),
            data: Bytes::new(&self.env),
        }
    }

    fn direct(&self, venue: &Address, amount: i128) -> Vec<Split> {
        vec![
            &self.env,
            Split {
                amount_in: amount,
                hops: vec![&self.env, self.hop(venue, &self.c)],
            },
        ]
    }

    pub(crate) fn desc(&self, amount: i128, min: i128) -> SwapDescription {
        SwapDescription {
            token_in: self.a.clone(),
            token_out: self.c.clone(),
            amount_in: amount,
            min_out: min,
            to: self.user.clone(),
        }
    }

    fn swap(
        &self,
        route: &Vec<Split>,
        amount: i128,
        min: i128,
        partner: &Option<Partner>,
    ) -> SwapResult {
        self.agg
            .swap(&self.user, &self.desc(amount, min), route, partner)
    }

    fn try_swap_err(
        &self,
        route: &Vec<Split>,
        amount: i128,
        min: i128,
        partner: &Option<Partner>,
    ) -> soroban_sdk::Error {
        self.agg
            .try_swap(&self.user, &self.desc(amount, min), route, partner)
            .err()
            .unwrap()
            .unwrap()
    }

    pub(crate) fn bal(&self, token: &Address, who: &Address) -> i128 {
        TokenClient::new(&self.env, token).balance(who)
    }
}

#[test]
fn a_direct_swap_pays_the_route_less_the_protocol_fee() {
    let f = setup();
    let v = f.venue(9_900, 0, &[&f.c]);
    let r = f.swap(&f.direct(&v, 1_000_000), 1_000_000, 989_010, &None);

    assert_eq!(r.gross_out, 990_000);
    assert_eq!(r.protocol_fee, 990);
    assert_eq!(r.amount_out, 989_010);
    assert_eq!(f.bal(&f.c, &f.user), 989_010);
    assert_eq!(f.agg.owed(&f.protocol, &f.c), 990);
    // The aggregator holds exactly what it owes, nothing more.
    assert_eq!(f.bal(&f.c, &f.agg.address), f.agg.total_owed(&f.c));
    assert_eq!(f.bal(&f.a, &f.agg.address), 0);
}

#[test]
fn a_route_splits_across_venues_and_hops_through_intermediates() {
    let f = setup();
    let direct = f.venue(9_900, 0, &[&f.c]);
    let to_b = f.venue(10_000, 0, &[&f.b]);
    let b_to_c = f.venue(9_950, 0, &[&f.c]);
    let route = vec![
        &f.env,
        Split {
            amount_in: 600_000,
            hops: vec![&f.env, f.hop(&direct, &f.c)],
        },
        Split {
            amount_in: 400_000,
            hops: vec![&f.env, f.hop(&to_b, &f.b), f.hop(&b_to_c, &f.c)],
        },
    ];
    let quoted = f.agg.quote_route(&f.a, &f.c, &1_000_000, &route, &None);
    let r = f.swap(&route, 1_000_000, quoted, &None);
    assert_eq!(r.gross_out, 594_000 + 398_000);
    assert_eq!(r.amount_out, quoted);
    assert_eq!(f.bal(&f.b, &f.agg.address), 0);
}

#[test]
fn a_partner_fee_is_split_with_the_protocol_and_both_claim() {
    let f = setup();
    let v = f.venue(10_000, 0, &[&f.c]);
    let partner = Address::generate(&f.env);
    let p = Some(Partner {
        address: partner.clone(),
        fee_bps: 50,
    });
    let r = f.swap(&f.direct(&v, 1_000_000), 1_000_000, 1, &p);

    // 0.1% protocol + 0.5% partner, of which the protocol keeps 20%.
    assert_eq!(r.protocol_fee, 1_000);
    assert_eq!(r.partner_fee, 5_000);
    assert_eq!(r.amount_out, 994_000);
    assert_eq!(f.agg.owed(&partner, &f.c), 4_000);
    assert_eq!(f.agg.owed(&f.protocol, &f.c), 2_000);

    f.agg.claim(&partner, &vec![&f.env, f.c.clone()]);
    f.agg.claim(&f.protocol, &vec![&f.env, f.c.clone()]);
    assert_eq!(f.bal(&f.c, &partner), 4_000);
    assert_eq!(f.bal(&f.c, &f.protocol), 2_000);
    assert_eq!(f.agg.total_owed(&f.c), 0);
    assert_eq!(f.bal(&f.c, &f.agg.address), 0);
}

#[test]
fn a_partner_without_a_trustline_cannot_break_swaps() {
    let f = setup();
    let v = f.venue(10_000, 0, &[&f.c]);
    // A real G... account that has no trustline for the output asset.
    let key = stellar_strkey::ed25519::PublicKey([5u8; 32]).to_string();
    let partner = Address::from_string(&String::from_str(&f.env, &key));
    let p = Some(Partner {
        address: partner.clone(),
        fee_bps: 50,
    });

    // The swap settles; the fee waits for the partner.
    f.swap(&f.direct(&v, 1_000_000), 1_000_000, 1, &p);
    assert_eq!(f.agg.owed(&partner, &f.c), 4_000);
    // Only the partner's own claim fails, until it adds a trustline.
    assert!(f
        .agg
        .try_claim(&partner, &vec![&f.env, f.c.clone()])
        .is_err());
}

#[test]
fn the_minimum_is_checked_net_of_every_fee() {
    let f = setup();
    let v = f.venue(9_900, 0, &[&f.c]);
    let before = f.bal(&f.a, &f.user);
    assert_eq!(
        f.try_swap_err(&f.direct(&v, 1_000_000), 1_000_000, 989_011, &None),
        Error::BelowMinOut.into()
    );
    assert_eq!(f.bal(&f.a, &f.user), before);
}

#[test]
fn a_venue_paying_less_than_it_quotes_is_measured_not_trusted() {
    let f = setup();
    let v = f.venue(9_900, 1_000, &[&f.c]);
    let quoted = f
        .agg
        .quote_route(&f.a, &f.c, &1_000_000, &f.direct(&v, 1_000_000), &None);
    // Holding the swap to the quote catches the shortfall.
    assert!(f
        .agg
        .try_swap(
            &f.user,
            &f.desc(1_000_000, quoted),
            &f.direct(&v, 1_000_000),
            &None
        )
        .is_err());
    // Fees are taken on what actually arrived.
    let r = f.swap(&f.direct(&v, 1_000_000), 1_000_000, 1, &None);
    assert_eq!(r.gross_out, 989_000);
}

#[test]
fn malformed_routes_are_rejected_before_anything_moves() {
    let f = setup();
    let v = f.venue(9_900, 0, &[&f.c]);
    let stranger = f.env.register(MockDex, (9_900i128, 0i128));
    let wrong_end = vec![
        &f.env,
        Split {
            amount_in: 1_000,
            hops: vec![&f.env, f.hop(&v, &f.b)],
        },
    ];
    let cases: [(Vec<Split>, i128, Error); 4] = [
        (f.direct(&v, 999), 1_000, Error::InvalidRoute),
        (wrong_end, 1_000, Error::InvalidRoute),
        (f.direct(&stranger, 1_000), 1_000, Error::VenueNotAllowed),
        (Vec::new(&f.env), 1_000, Error::InvalidRoute),
    ];
    for (route, amount, err) in cases {
        assert_eq!(f.try_swap_err(&route, amount, 1, &None), err.into());
    }
    assert_eq!(f.bal(&f.a, &f.user), BIG);
}

#[test]
fn a_partner_fee_above_the_cap_is_refused() {
    let f = setup();
    let v = f.venue(9_900, 0, &[&f.c]);
    let p = Some(Partner {
        address: Address::generate(&f.env),
        fee_bps: 101,
    });
    assert_eq!(
        f.try_swap_err(&f.direct(&v, 1_000), 1_000, 1, &p),
        Error::PartnerFeeTooHigh.into()
    );
}

#[test]
fn router_fills_execute_the_encoded_route_and_cannot_spend_owed_fees() {
    let f = setup();
    let v = f.venue(9_900, 0, &[&f.c]);
    // Fees in `c` accrue first.
    f.swap(&f.direct(&v, 1_000_000), 1_000_000, 1, &None);
    let owed = f.agg.total_owed(&f.c);

    // Routing c -> a: the router never pushed any `c`, so the owed fees must
    // not be mistaken for its input.
    let back = f.venue(10_000, 0, &[&f.a]);
    let route = vec![
        &f.env,
        Split {
            amount_in: 500,
            hops: vec![&f.env, f.hop(&back, &f.a)],
        },
    ];
    let recipient = Address::generate(&f.env);
    assert_eq!(
        f.agg
            .try_fill(
                &recipient,
                &f.c,
                &f.a,
                &500,
                &1,
                &Route::Splits(route.clone()).to_xdr(&f.env)
            )
            .err()
            .unwrap()
            .unwrap(),
        Error::InputShortfall.into()
    );
    assert_eq!(f.agg.total_owed(&f.c), owed);

    // Pushed for real, it settles, net of the protocol fee only.
    StellarAssetClient::new(&f.env, &f.c).mint(&f.agg.address, &500);
    let net = f.agg.fill(
        &recipient,
        &f.c,
        &f.a,
        &500,
        &1,
        &Route::Splits(route).to_xdr(&f.env),
    );
    assert_eq!(net, 500);
    assert_eq!(f.bal(&f.a, &recipient), 500);
}

#[test]
fn a_pause_stops_swaps_but_never_claims() {
    let f = setup();
    let v = f.venue(9_900, 0, &[&f.c]);
    f.swap(&f.direct(&v, 1_000_000), 1_000_000, 1, &None);
    f.agg.set_paused(&true);
    assert_eq!(
        f.try_swap_err(&f.direct(&v, 1_000), 1_000, 1, &None),
        Error::Paused.into()
    );
    f.agg.claim(&f.protocol, &vec![&f.env, f.c.clone()]);
    assert_eq!(f.bal(&f.c, &f.protocol), 990);
}

#[test]
fn the_router_quote_is_the_best_direct_venue_net_of_fee() {
    let f = setup();
    f.venue(9_900, 0, &[&f.c]);
    f.venue(9_950, 0, &[&f.c]);
    assert_eq!(f.agg.quote(&f.a, &f.c, &1_000_000), 995_000 - 995);
}

#[test]
fn governance_is_admin_only_and_bounded() {
    let f = setup();
    let fees = FeeConfig {
        recipient: f.protocol.clone(),
        protocol_fee_bps: 101,
        max_partner_fee_bps: 100,
        partner_share_bps: 2_000,
    };
    assert_eq!(
        f.agg.try_set_fees(&fees).err().unwrap().unwrap(),
        Error::InvalidConfig.into()
    );
    f.env.set_auths(&[]);
    assert!(f
        .agg
        .try_allow_venue(&Address::generate(&f.env), &true)
        .is_err());
}
