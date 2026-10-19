//! Invariant sweeps over randomised routes, partners and minimums.

use crate::test::{setup, Fixture, BIG};
use crate::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::vec;
extern crate std;
use std::vec::Vec as StdVec;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
    fn range(&mut self, lo: u64, hi: u64) -> i128 {
        (lo + self.next() % (hi - lo + 1)) as i128
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.range(0, items.len() as u64 - 1) as usize]
    }
}

struct World {
    f: Fixture,
    venues: StdVec<Address>,
    partners: StdVec<Address>,
}

fn world() -> World {
    let f = setup();
    let mut rng = Rng(21);
    let venues = (0..4)
        .map(|_| {
            let rate = rng.range(9_000, 11_000);
            let shortfall = rng.range(0, 3);
            f.venue(rate, shortfall, &[&f.b, &f.c])
        })
        .collect();
    let partners = (0..3).map(|_| Address::generate(&f.env)).collect();
    World {
        f,
        venues,
        partners,
    }
}

impl World {
    /// 1–3 splits, each direct or through `b`; sometimes deliberately malformed.
    fn route(&self, rng: &mut Rng, amount: i128) -> (Vec<Split>, bool) {
        let f = &self.f;
        let n = rng.range(1, 3);
        let mut route = Vec::new(&f.env);
        let mut left = amount;
        for i in 0..n {
            let part = if i == n - 1 {
                left
            } else {
                rng.range(1, (left / 2).max(1) as u64)
            };
            left -= part;
            let hops = if rng.range(0, 1) == 0 {
                vec![&f.env, f.hop(rng.pick(&self.venues), &f.c)]
            } else {
                vec![
                    &f.env,
                    f.hop(rng.pick(&self.venues), &f.b),
                    f.hop(rng.pick(&self.venues), &f.c),
                ]
            };
            route.push_back(Split {
                amount_in: part,
                hops,
            });
        }
        let malformed = rng.range(0, 7) == 0;
        if malformed {
            let mut s = route.get(0).unwrap();
            if rng.range(0, 1) == 0 {
                s.amount_in += 1;
            } else {
                s.hops = vec![&f.env, f.hop(rng.pick(&self.venues), &f.b)];
            }
            route.set(0, s);
        }
        (route, malformed)
    }

    fn assert_owed_is_held(&self) {
        let f = &self.f;
        for token in [&f.a, &f.b, &f.c] {
            let total = f.agg.total_owed(token);
            let mut sum = f.agg.owed(&f.protocol, token);
            for p in &self.partners {
                sum += f.agg.owed(p, token);
            }
            assert_eq!(sum, total, "owed entries do not add up");
            assert!(f.bal(token, &f.agg.address) >= total, "owed fees spent");
        }
    }
}

impl World {
    /// One random swap; returns whether it executed. Either the user gets at
    /// least the minimum, net, for exactly `amount`, or nothing moves.
    fn swap_once(&self, rng: &mut Rng) -> bool {
        let f = &self.f;
        let amount = rng.range(1_000, 10_000_000);
        let (route, malformed) = self.route(rng, amount);
        let partner = match rng.range(0, 2) {
            0 => None,
            _ => Some(Partner {
                address: rng.pick(&self.partners).clone(),
                fee_bps: rng.range(0, 150) as u32,
            }),
        };
        let quoted = match f.agg.try_quote_route(&f.a, &f.c, &amount, &route, &partner) {
            Ok(Ok(q)) => q,
            _ => 0,
        };
        let min = rng.range(1, (quoted.max(1) as u64) * 11 / 10);
        let before = (f.bal(&f.a, &f.user), f.bal(&f.c, &f.user));
        let Ok(Ok(r)) = f
            .agg
            .try_swap(&f.user, &f.desc(amount, min), &route, &partner)
        else {
            assert_eq!((f.bal(&f.a, &f.user), f.bal(&f.c, &f.user)), before);
            return false;
        };
        assert!(!malformed, "a malformed route executed");
        assert!(r.amount_out >= min);
        assert_eq!(f.bal(&f.c, &f.user) - before.1, r.amount_out);
        assert_eq!(before.0 - f.bal(&f.a, &f.user), amount);
        true
    }
}

#[test]
fn inv_swaps_pay_the_net_minimum_and_never_spend_owed_fees() {
    let w = world();
    let f = &w.f;
    let mut rng = Rng(22);
    let (mut ok, mut reverted) = (0, 0);
    for _ in 0..120 {
        if w.swap_once(&mut rng) {
            ok += 1;
        } else {
            reverted += 1;
        }
        if rng.range(0, 4) == 0 {
            let who = if rng.range(0, 1) == 0 {
                f.protocol.clone()
            } else {
                rng.pick(&w.partners).clone()
            };
            f.agg
                .claim(&who, &vec![&f.env, f.a.clone(), f.b.clone(), f.c.clone()]);
        }
        w.assert_owed_is_held();
    }
    // The sweep exercised both outcomes.
    assert!(ok > 20 && reverted > 10, "ok {ok}, reverted {reverted}");
    assert!(f.bal(&f.a, &f.user) < BIG);
}
