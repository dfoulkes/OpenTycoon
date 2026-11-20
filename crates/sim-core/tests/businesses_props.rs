//! Property tests for businesses invariants over random days of trading (docs/engineering.md
//! §1): stock is never negative and never above capacity, the 06:00 stock bill always equals
//! what is delivered, every unit delivered is either sold,
//! written off or still on the shelf, and sales revenue equals units × shelf price.
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test code")]

mod common;

use proptest::prelude::*;
use sim_core::businesses::{BuildRequest, BusinessId, Businesses, FitOutLevel};
use sim_core::clock::{Clock, ClockRules};
use sim_core::economy::{AccountId, PlayerId, settlement};
use sim_core::land::{BlockId, Footprint, Map};
use sim_core::money::{Money, Rate};

#[derive(Debug, Clone)]
enum Op {
    Order(String, i64),
    Price(String, i64),
    Sell(usize, String, i64),
    Toggle(usize, bool),
    Ticks(u32),
}

fn op() -> impl Strategy<Value = Op> {
    let good = prop_oneof![Just("bread".to_owned()), Just("drinks".to_owned())];
    prop_oneof![
        (good.clone(), -10i64..200).prop_map(|(g, q)| Op::Order(g, q)),
        (good.clone(), -100i64..40_000).prop_map(|(g, p)| Op::Price(g, p)),
        (0usize..2, good, -2i64..60).prop_map(|(s, g, q)| Op::Sell(s, g, q)),
        (0usize..2, any::<bool>()).prop_map(|(s, o)| Op::Toggle(s, o)),
        (0u32..20_000).prop_map(Op::Ticks),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn stock_stays_within_zero_and_capacity_and_is_conserved(ops in prop::collection::vec(op(), 1..60)) {
        let config = common::shipped_config();
        let map = Map::from_config(&config.map);
        let mut clock = Clock::new(ClockRules::from_config(&config.game.clock));
        let mut shops = Businesses::default();
        let owner = AccountId::Player(PlayerId(1));
        let mut ids: Vec<BusinessId> = Vec::new();
        for (type_id, block) in [("bakery", 0), ("bar", 3)] {
            let (id, _) = shops
                .build(
                    BuildRequest {
                        type_id: type_id.into(),
                        block: BlockId(block),
                        footprint: Footprint { x: 0, y: 0, w: 3, d: 3 },
                        height: 10,
                        owner,
                        fit_out: FitOutLevel::Medium,
                    },
                    &config,
                    &map,
                    clock.year(),
                )
                .unwrap();
            ids.push(id);
        }
        let (mut delivered, mut sold, mut binned) = (0i64, 0i64, 0i64);
        let mut revenue = Money::ZERO;
        for op in ops {
            match op {
                Op::Order(good, q) => {
                    for id in &ids {
                        let _ = shops.set_order(*id, &good, q);
                    }
                }
                Op::Price(good, bp) => {
                    for id in &ids {
                        let _ = shops.set_price(*id, &good, Rate::from_bp(bp));
                    }
                }
                Op::Sell(i, good, q) => {
                    let id = ids[i];
                    let before = shops.get(id).unwrap().stock_of(&good);
                    let price = shops.get(id).unwrap().unit_price(&good, &config.businesses);
                    if let Ok(m) = shops.try_sell(id, &good, q, &clock, &config.businesses) {
                        prop_assert_eq!(m, price.unwrap().saturating_mul(q));
                        prop_assert_eq!(shops.get(id).unwrap().stock_of(&good), before - q);
                        sold += q;
                        revenue += m;
                    } else {
                        prop_assert_eq!(shops.get(id).unwrap().stock_of(&good), before);
                    }
                }
                Op::Toggle(i, open) => shops.set_open(ids[i], open).unwrap(),
                Op::Ticks(n) => {
                    for _ in 0..n {
                        let fired = clock.advance();
                        if fired.midnight {
                            binned += shops.midnight().iter().map(|w| w.units).sum::<i64>();
                            revenue = Money::ZERO;
                        }
                        if fired.settlement {
                            let before: i64 = shops.by_id.values().flat_map(|b| b.stock.values()).sum();
                            let billed: Money = shops
                                .by_id
                                .values()
                                .map(|b| settlement::stock_order_bill(b, &config).amount)
                                .sum();
                            let delivered_value: Money = shops
                                .deliver_orders()
                                .iter()
                                .map(|d| config.businesses.goods[&d.good].purchase_price.money().saturating_mul(d.units))
                                .sum();
                            prop_assert_eq!(billed, delivered_value, "billed and delivered diverged");
                            let after: i64 = shops.by_id.values().flat_map(|b| b.stock.values()).sum();
                            delivered += after - before;
                        }
                    }
                }
            }
            let mut takings = Money::ZERO;
            for b in shops.by_id.values() {
                takings += b.takings_today;
                for units in b.stock.values() {
                    prop_assert!(*units >= 0, "negative stock");
                    prop_assert!(*units <= b.capacity_per_good, "over capacity");
                }
                for units in b.standing_order.values().chain(b.tomorrow_order.values()) {
                    prop_assert!((0..=b.capacity_per_good).contains(units));
                }
            }
            prop_assert_eq!(takings, revenue);
            let on_hand: i64 = shops.by_id.values().flat_map(|b| b.stock.values()).sum();
            prop_assert_eq!(delivered, sold + binned + on_hand);
        }
    }
}

/// docs/economy.md "Business stats": every shipped stock density is the formula's, rounded up
/// to three significant figures (so at most 1% above it). A change to rents, utility rates or
/// prices that isn't carried through to the densities fails here.
#[test]
fn shipped_stock_densities_follow_the_formula() {
    use sim_core::businesses::formula::derived_stock_density;
    let config = common::shipped_config();
    let mut checked = 0;
    for (id, kind) in &config.businesses.types {
        let Some(derived) = derived_stock_density(kind, &config) else {
            assert!(
                kind.goods.is_empty(),
                "{id}: its goods lose money at the reference sell-through"
            );
            continue;
        };
        let shipped = kind.stock_density.rate().bp();
        let want = derived.bp();
        assert!(
            shipped >= want && shipped * 100 <= want * 101,
            "{id}: stock_density {shipped} bp but the formula gives {want} bp (regenerate, \
             docs/economy.md)"
        );
        checked += 1;
    }
    assert!(checked > 50, "only {checked} types checked");
}
