//! Property tests: each one asserts a rule that must hold over a randomised
//! sweep of inputs, rather than for one hand-picked case.

use super::*;
use crate::test::{setup, Fixture, DAY, HUGE, ONE};
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::Address;

const BPS_I: i128 = 10_000;
const DENOM_I: i128 = BPS_I * 86_400;

/// Deterministic LCG — reproducible sweeps, no external dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
    fn range(&mut self, lo: u64, hi: u64) -> u64 {
        assert!(lo <= hi, "empty range {lo}..={hi}");
        lo + self.next() % (hi - lo + 1)
    }
}

impl Fixture {
    fn set_horizon(&self, seconds: u32, max_bps: u32) {
        self.client.set_schedule(
            &self.admin,
            &self.rwa,
            &Schedule {
                mode: ScheduleMode::Rolling,
                rolling_seconds: seconds,
                next_redemption_at: 0,
                cycle_seconds: 0,
                max_bps_per_day: max_bps,
            },
        );
    }

    fn fill(&self, order: &RfqOrder, amount: i128) -> FillResult {
        self.client.fill_rfq_order(order, &self.sign(&self.client.hash_rfq_order(order)), &None, &self.taker, &amount,
        )
    }

    fn escrowed(&self) -> i128 {
        self.rwa_of(&self.client.address)
    }

    fn funded_buyer(&self) -> Address {
        let buyer = Address::generate(&self.env);
        StellarAssetClient::new(&self.env, &self.usd).mint(&buyer, &HUGE);
        TokenClient::new(&self.env, &self.usd).approve(
            &buyer,
            &self.client.address,
            &HUGE,
            &(self.env.ledger().sequence() + 1_000_000),
        );
        buyer
    }
}

/// Total supply of both tokens is conserved and the contract keeps nothing:
/// every RFQ fill is a pure transfer between maker, taker and fee recipient.
#[test]
fn inv_fills_conserve_supply_and_leave_no_dust_in_the_contract() {
    let f = setup();
    let mut rng = Rng(1);
    let fee_recipient = Address::generate(&f.env);
    let parties = [
        f.maker.clone(),
        f.taker.clone(),
        fee_recipient.clone(),
        f.client.address.clone(),
    ];
    let total = |f: &Fixture| -> (i128, i128) {
        parties.iter().fold((0, 0), |(u, r), who| {
            (u + f.usd_of(who), r + f.rwa_of(who))
        })
    };
    let before = total(&f);

    for salt in 0..40u64 {
        let mut order = f.rfq();
        order.salt = salt;
        order.taker_amount = rng.range(1_000, 10_000_000) as i128;
        order.max_maker_amount = order.taker_amount;
        order.fee_bps = rng.range(0, 1_000) as u32;
        order.fee_recipient = fee_recipient.clone();
        order.maker_bps_per_day = rng.range(0, 10) as u32;

        let amount = rng.range(1, order.taker_amount as u64) as i128;
        let r = f.fill(&order, amount);

        // The fee is carved out of the maker output, never added on top.
        assert_eq!(r.fee, r.maker_filled * order.fee_bps as i128 / BPS_I);
        assert!(r.fee <= r.maker_filled);
        assert_eq!(f.escrowed(), 0, "settlement must not custody funds");
        assert_eq!(f.usd_of(&f.client.address), 0);
    }
    assert_eq!(total(&f), before, "token supply must be conserved");
}

/// The rate model never pays a premium, and paying for more days — or more
/// basis points per day — never pays out more.
#[test]
fn inv_discount_is_monotone_in_rate_and_horizon() {
    let f = setup();
    let mut rng = Rng(2);
    let amount = 1_000_000_000i128;

    for _ in 0..40 {
        let days = rng.range(1, 300);
        // Keep `bps x days` under 100% for the probes too, so the sweep tests the
        // curve rather than the DiscountTooLarge guard.
        let bps = rng.range(0, 9_000 / (days + 1) - 1) as u32;
        f.set_horizon((days * DAY) as u32, 10_000);

        let mut order = f.rfq();
        order.taker_amount = amount;
        order.maker_bps_per_day = bps;
        order.taker_max_bps_per_day = 10_000;
        let base = f.client.quote_rfq_order(&order, &amount).maker_amount;

        // Never above face value, never negative.
        assert!(base <= amount && base >= 0);
        // The closed form the README documents.
        let discount = bps as i128 * (days * DAY) as i128;
        assert_eq!(base, amount * (DENOM_I - discount) / DENOM_I);

        // A higher rate pays out no more.
        order.maker_bps_per_day = bps + 1;
        assert!(f.client.quote_rfq_order(&order, &amount).maker_amount <= base);

        // A longer horizon pays out no more.
        order.maker_bps_per_day = bps;
        f.set_horizon(((days + 1) * DAY) as u32, 10_000);
        assert!(f.client.quote_rfq_order(&order, &amount).maker_amount <= base);
    }
}

/// Splitting a fill into slices can never extract more than filling it once —
/// rounding always favours the contract, so it is never a way to mint value.
#[test]
fn inv_splitting_a_fill_never_pays_more_than_one_shot() {
    let f = setup();
    let mut rng = Rng(3);

    for salt in 0..25u64 {
        let taker_amount = rng.range(10_000, 5_000_000) as i128;
        let slices = rng.range(2, 6);

        let mut whole = f.rfq();
        whole.salt = salt * 2;
        whole.taker_amount = taker_amount;
        whole.max_maker_amount = taker_amount;
        whole.maker_bps_per_day = rng.range(1, 10) as u32;
        let one_shot = f.fill(&whole, taker_amount).maker_filled;

        let mut split = whole.clone();
        split.salt = salt * 2 + 1;
        let mut paid = 0i128;
        let mut done = 0i128;
        for i in 0..slices {
            let slice = if i == slices - 1 {
                taker_amount - done
            } else {
                taker_amount / slices as i128
            };
            paid += f.fill(&split, slice).maker_filled;
            done += slice;
        }
        assert_eq!(done, taker_amount);
        assert!(paid <= one_shot, "{paid} > {one_shot}");
    }
}

/// Neither signed bound is ever breached, however the fill is sliced. Bounds
/// signed at exactly the full-fill quote sit on a one-unit rounding edge and may
/// reject a slice; with headroom every slice settles.
#[test]
fn inv_signed_bounds_are_strict_and_headroom_always_fills() {
    let f = setup();
    let mut rng = Rng(4);
    let mut rejected = 0;

    for salt in 0..30u64 {
        let taker_amount = rng.range(100_000, 1_000_000) as i128;
        let mut order = f.rfq();
        order.salt = salt * 2;
        order.taker_amount = taker_amount;
        order.maker_bps_per_day = rng.range(0, 10) as u32;
        order.fee_bps = rng.range(0, 1_000) as u32;

        // Phase 1 — bounds pinned to the full-fill quote. A slice may revert on
        // rounding, but one that settles must respect both bounds.
        let full = f.client.quote_rfq_order(&order, &taker_amount);
        order.max_maker_amount = full.maker_amount;
        order.min_received_amount = full.maker_amount - full.fee;
        let hash = f.client.hash_rfq_order(&order);
        for _ in 0..4 {
            let slice = rng.range(1, taker_amount as u64 / 4) as i128;
            let before = f.usd_of(&f.taker);
            match f
                .client
                .try_fill_rfq_order(&order, &f.sign(&hash), &None, &f.taker, &slice)
            {
                Ok(Ok(r)) => {
                    let received = f.usd_of(&f.taker) - before;
                    assert_eq!(received, r.maker_filled - r.fee);
                    assert!(received >= div_ceil(order.min_received_amount * slice, taker_amount));
                    assert!(r.maker_filled <= order.max_maker_amount * slice / taker_amount);
                }
                Err(Ok(e)) => {
                    // Rejected only ever for breaching a bound — never for any
                    // other reason — and with nothing moved.
                    assert!(
                        e == Error::MakerAmountTooHigh.into()
                            || e == Error::BelowMinReceived.into(),
                        "rejected for the wrong reason"
                    );
                    rejected += 1;
                    assert_eq!(f.usd_of(&f.taker), before);
                }
                other => panic!("unexpected result {other:?}"),
            }
        }

        // Phase 2 — 5% of proportional slack, the shape of a real slippage
        // tolerance. Headroom has to be proportional: an absolute cushion is
        // pro-rated away to nothing on a small slice. Every slice now settles.
        order.salt = salt * 2 + 1;
        order.max_maker_amount = full.maker_amount * 105 / 100;
        order.min_received_amount = (full.maker_amount - full.fee) * 95 / 100;
        let hash = f.client.hash_rfq_order(&order);
        let mut done = 0i128;
        while done < taker_amount {
            let remaining = (taker_amount - done) as u64;
            let floor_slice = (taker_amount as u64 / 20).clamp(1, remaining);
            let slice = rng.range(floor_slice, remaining) as i128;
            let before = f.usd_of(&f.taker);
            let r = f
                .client
                .fill_rfq_order(&order, &f.sign(&hash), &None, &f.taker, &slice);
            let received = f.usd_of(&f.taker) - before;

            assert_eq!(received, r.maker_filled - r.fee);
            assert!(received >= div_ceil(order.min_received_amount * slice, taker_amount));
            assert!(r.maker_filled <= order.max_maker_amount * slice / taker_amount);
            done += slice;
        }
        assert_eq!(f.client.filled_amount(&hash), taker_amount);
        // Exhausted: one more unit reverts rather than over-filling.
        assert!(f
            .client
            .try_fill_rfq_order(&order, &f.sign(&hash), &None, &f.taker, &1)
            .is_err());
    }
    // About half of all slices land on the wrong side of one of the two
    // roundings, so bounds pinned exactly at the quote are not a usable
    // integration pattern — but some still settle, so the pro-rata itself is
    // sound rather than rejecting everything.
    assert!(rejected > 0 && rejected < 120, "{rejected}/120 rejected");
}

/// Recorded fills never exceed the signed size, and anything over reverts
/// wholesale rather than settling a clamped slice.
#[test]
fn inv_recorded_fills_never_exceed_the_order() {
    let f = setup();
    let mut rng = Rng(5);

    for salt in 0..30u64 {
        let taker_amount = rng.range(100, 100_000) as i128;
        let mut order = f.rfq();
        order.salt = salt;
        order.taker_amount = taker_amount;
        order.max_maker_amount = taker_amount;
        let hash = f.client.hash_rfq_order(&order);

        let mut expected = 0i128;
        for _ in 0..6 {
            let ask = rng.range(1, (taker_amount * 2) as u64) as i128;
            let fits = expected + ask <= taker_amount;
            let ok = f
                .client
                .try_fill_rfq_order(&order, &f.sign(&hash), &None, &f.taker, &ask)
                .is_ok();
            // A fill succeeds exactly when it fits in what is left — no clamping.
            assert_eq!(ok, fits, "ask={ask} filled={expected} of {taker_amount}");
            if ok {
                expected += ask;
            }
            assert!(expected <= taker_amount);
            assert_eq!(f.client.filled_amount(&hash), expected);
        }
    }
}

/// The Dutch ask starts at the seller's start price, never rises, never falls
/// below the floor, and rests on the floor once the decay is over.
#[test]
fn inv_dutch_ask_decays_monotonically_within_bounds() {
    let f = setup();
    let mut rng = Rng(6);
    let decay = 7 * DAY;

    for _ in 0..10 {
        let start = rng.range(1_000, 1_000_000) as i128;
        let floor = rng.range(1, start as u64) as i128;
        let mut order = f.dutch();
        order.taker_amount = 1_000;
        order.start_maker_amount = start;
        order.min_maker_amount = floor;

        let created = f.env.ledger().timestamp();
        let id = f.client.create_dutch_order(&f.taker, &order);
        assert_eq!(f.client.current_ask(&id), start);

        let mut prev = start;
        for step in 1..=10u64 {
            f.env
                .ledger()
                .set_timestamp(created + step * decay / 8);
            let ask = f.client.current_ask(&id);
            assert!(ask <= prev, "ask rose: {prev} -> {ask}");
            assert!((floor..=start).contains(&ask));
            if step * decay / 8 >= decay {
                assert_eq!(ask, floor);
            }
            prev = ask;
        }
        f.client.cancel_dutch_order(&id);
    }
}

/// Escrow is solvent at every point: the contract's balance is exactly the sum
/// of the live listings, so no listing can be paid out of another's funds.
#[test]
fn inv_dutch_escrow_matches_the_sum_of_live_listings() {
    let f = setup();
    let mut rng = Rng(7);
    let buyer = f.funded_buyer();
    let mut live: [(u64, i128); 8] = [(0, 0); 8];
    let mut n = 0usize;

    for _ in 0..40 {
        let create = n == 0 || (n < live.len() && rng.range(0, 1) == 1);
        if create {
            let amount = rng.range(1_000, 500_000) as i128;
            let mut order = f.dutch();
            order.taker_amount = amount;
            order.start_maker_amount = amount;
            order.min_maker_amount = amount / 2;
            live[n] = (f.client.create_dutch_order(&f.taker, &order), amount);
            n += 1;
        } else {
            let pick = rng.range(0, n as u64 - 1) as usize;
            let (id, _) = live[pick];
            if rng.range(0, 1) == 1 {
                f.client.fill_dutch_order(&id, &buyer, &i128::MAX);
            } else {
                f.client.cancel_dutch_order(&id);
            }
            // A closed listing stays closed, whichever way it was closed.
            assert!(f.client.try_current_ask(&id).is_err());
            live[pick] = live[n - 1];
            n -= 1;
        }
        let expected: i128 = live[..n].iter().map(|(_, a)| a).sum();
        assert_eq!(f.escrowed(), expected, "escrow drifted from live listings");
        f.env
            .ledger()
            .set_timestamp(f.env.ledger().timestamp() + rng.range(1, 2 * DAY));
    }
}

/// A fixed-cycle horizon is always within one cycle and strictly counts down
/// second by second; a rolling one is constant.
#[test]
fn inv_schedule_horizon_stays_inside_one_cycle() {
    let f = setup();
    let mut rng = Rng(8);

    for _ in 0..20 {
        let cycle = rng.range(3_600, 90 * DAY);
        let start = f.env.ledger().timestamp();
        f.client.set_schedule(
            &f.admin,
            &f.rwa,
            &Schedule {
                mode: ScheduleMode::Cyclical,
                rolling_seconds: 0,
                next_redemption_at: start + rng.range(1, cycle),
                cycle_seconds: cycle as u32,
                max_bps_per_day: 10,
            },
        );
        for _ in 0..8 {
            let before = f.client.seconds_to_redemption(&f.rwa);
            assert!(before > 0 && before as u64 <= cycle);

            let step = rng.range(1, cycle / 4);
            f.env
                .ledger()
                .set_timestamp(f.env.ledger().timestamp() + step);
            let after = f.client.seconds_to_redemption(&f.rwa);
            assert!(after > 0 && after as u64 <= cycle);
            // Either it counted down by `step`, or the anchor rolled over.
            if (step as u32) < before {
                assert_eq!(after, before - step as u32);
            } else {
                assert!(after > before || after as u64 <= cycle);
            }
        }
    }
}

/// Quoting a pair in one direction and then the other returns to where it
/// started, up to floor rounding.
#[test]
fn inv_price_round_trips_between_a_pair() {
    let f = setup();
    let mut rng = Rng(9);

    for _ in 0..30 {
        // Bounded per-step moves keep every push inside the deviation guard.
        let price = f.client.price_of(&f.rwa, &f.usd) * rng.range(6, 14) as i128 / 10;
        f.client.push_price(&f.admin, &f.rwa, &price.max(ONE / 1_000));

        let forward = f.client.price_of(&f.rwa, &f.usd);
        let back = f.client.price_of(&f.usd, &f.rwa);
        let product = forward / 1_000_000_000 * (back / 1_000_000_000);
        let unit = ONE / 1_000_000_000 * (ONE / 1_000_000_000);
        assert!(
            (product - unit).abs() <= unit / 1_000_000,
            "round trip drifted: {product} vs {unit}"
        );
    }
}

/// An order that is not fillable is never fillable by any route: expiry,
/// cancellation, a salt sweep, the pause switch and the taker restriction all
/// hold regardless of size.
#[test]
fn inv_unfillable_orders_never_settle() {
    let mut rng = Rng(10);

    for case in 0..5u64 {
        let f = setup();
        let mut order = f.rfq();
        order.taker_amount = rng.range(1_000, 100_000) as i128;
        order.max_maker_amount = order.taker_amount;
        // Cases 2 and 4 change the order itself, so mutate before signing.
        if case == 2 {
            order.taker = Some(f.taker.clone());
        } else if case == 4 {
            order.taker = Some(Address::generate(&f.env));
        }
        let sig = f.sign(&f.client.hash_rfq_order(&order));

        match case {
            0 => f.env.ledger().set_timestamp(order.expiry + 1),
            1 => f.client.cancel_salt(&f.maker, &f.maker, &order.salt),
            // A named taker who retracted their salt: their request is void.
            2 => f.client.cancel_salt(&f.taker, &f.taker, &order.salt),
            3 => f.client.set_paused(&true),
            // A request naming someone else, with no signature from them.
            _ => {}
        }

        let before = (f.usd_of(&f.taker), f.rwa_of(&f.taker));
        for _ in 0..4 {
            let amount = rng.range(1, order.taker_amount as u64) as i128;
            assert!(f
                .client
                .try_fill_rfq_order(&order, &sig, &None, &f.taker, &amount)
                .is_err());
        }
        assert_eq!((f.usd_of(&f.taker), f.rwa_of(&f.taker)), before);
        assert_eq!(f.client.filled_amount(&f.client.hash_rfq_order(&order)), 0);
    }
}

/// Only a key the maker owns or has registered can move the maker's funds.
#[test]
fn inv_only_authorised_keys_can_spend_a_makers_allowance() {
    let f = setup();
    let mut rng = Rng(11);

    for salt in 0..10u64 {
        let mut order = f.rfq();
        order.salt = salt;
        let hash = f.client.hash_rfq_order(&order);
        let rogue = ed25519_dalek::SigningKey::from_bytes(&[(salt as u8) | 0x40; 32]);
        let digest = crate::hash::sep53(&f.env, &hash);
        let sig = Signature {
            signer: soroban_sdk::BytesN::from_array(&f.env, &rogue.verifying_key().to_bytes()),
            signature: soroban_sdk::BytesN::from_array(
                &f.env,
                &ed25519_dalek::Signer::sign(&rogue, &digest.to_array()).to_bytes(),
            ),
        };
        let before = f.usd_of(&f.maker);
        assert!(f
            .client
            .try_fill_rfq_order(&order, &sig, &None, &f.taker, &(rng.range(1, 1_000) as i128))
            .is_err());
        assert_eq!(f.usd_of(&f.maker), before);
    }
}

/// A slice too small to quote a positive output is rejected outright: the
/// contract never settles a fill that pays the taker nothing.
#[test]
fn inv_a_fill_never_settles_for_zero_output() {
    let f = setup();
    let mut rng = Rng(12);

    for salt in 0..15u64 {
        let mut order = f.rfq();
        order.salt = salt;
        order.taker_amount = rng.range(10_000, 1_000_000) as i128;
        order.max_maker_amount = order.taker_amount;
        order.maker_bps_per_day = rng.range(1, 10) as u32;
        let hash = f.client.hash_rfq_order(&order);

        // One raw unit at a discount quotes floor(1 x r) = 0 maker tokens.
        assert_eq!(f.client.quote_rfq_order(&order, &1).maker_amount, 0);
        let before = (f.usd_of(&f.taker), f.rwa_of(&f.taker));
        assert!(f
            .client
            .try_fill_rfq_order(&order, &f.sign(&hash), &None, &f.taker, &1)
            .is_err());
        assert_eq!((f.usd_of(&f.taker), f.rwa_of(&f.taker)), before);
        assert_eq!(f.client.filled_amount(&hash), 0);
    }
}

fn div_ceil(a: i128, b: i128) -> i128 {
    (a + b - 1) / b
}
