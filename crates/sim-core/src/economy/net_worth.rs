//! Net worth, the win metric (§3; R19, R31, R39, with property value = build cost from R41).
//!
//! ```text
//! net_worth = cash + stock
//!           + Σ (owned properties) property_value
//!           + Σ (held lots) [ land_value + uplift × Σ property_value on that lot ]
//! land_value     = last auction price × remaining lease years / lease term
//! property_value = build cost (buildings don't wear, R41)
//! stock          = units on hand × purchase price
//! ```
//!
//! It is recalculated every midnight **before** the stock write-off (R31), so the last
//! midnight's figure, stock included, is the final score (R19). Utilities add nothing directly:
//! they count through the cash they earn and save (R19).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{AccountId, Ledger, PlayerId};
use crate::businesses::Businesses;
use crate::config::businesses::Good;
use crate::land::Map;
use crate::money::{Money, Rate};

/// The parts of one player's net worth (R19, R39), all already valued.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetWorthInputs {
    /// Cash balance.
    pub cash: Money,
    /// Stock on hand at purchase price (before the midnight write-off, R31).
    pub stock: Money,
    /// Value of each property the player owns (build cost, R41).
    pub owned_property_values: Vec<Money>,
    /// For each lot the player leases: its land value and the values of every property on it,
    /// whoever owns them.
    pub held_lots: Vec<HeldLot>,
}

/// A leased lot's contribution to net worth.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeldLot {
    /// `last auction price × remaining lease years / term` (§3).
    pub land_value: Money,
    /// Value of every property on the lot.
    pub property_values: Vec<Money>,
}

/// `net_worth = cash + stock + Σ owned property + Σ held lots (land + uplift × Σ property)`
/// (§3, R19, R39). Utilities add nothing directly (R19).
pub fn net_worth(inputs: &NetWorthInputs, lot_property_uplift: Rate) -> Money {
    let owned: Money = inputs.owned_property_values.iter().copied().sum();
    let lots: Money = inputs
        .held_lots
        .iter()
        .map(|lot| {
            let on_lot: Money = lot.property_values.iter().copied().sum();
            lot.land_value + lot_property_uplift.apply(on_lot)
        })
        .sum();
    inputs.cash + inputs.stock + owned + lots
}

/// `land_value = last_price × remaining_years / term_years` (§3), rounded half away from zero
/// to the penny. A zero term values the land at nothing.
pub fn land_value(last_price: Money, remaining_years: u32, term_years: u32) -> Money {
    if term_years == 0 {
        return Money::ZERO;
    }
    let num = i128::from(last_price.pence()) * i128::from(remaining_years);
    let den = i128::from(term_years);
    let half = den / 2;
    let rounded = if num >= 0 {
        (num + half) / den
    } else {
        (num - half) / den
    };
    Money::from_pence(i64::try_from(rounded).unwrap_or(if rounded > 0 {
        i64::MAX
    } else {
        i64::MIN
    }))
}

/// `stock = Σ units on hand × purchase price` (§3). Goods missing from the catalogue (which
/// config validation rules out) and negative counts are valued at nothing.
pub fn stock_value(stock: &BTreeMap<String, i64>, goods: &BTreeMap<String, Good>) -> Money {
    stock
        .iter()
        .filter_map(|(good, units)| {
            let price = goods.get(good)?.purchase_price.money();
            Some(price.saturating_mul((*units).max(0)))
        })
        .sum()
}

/// Gathers one account's [`NetWorthInputs`] from the world's state, valuing leases at
/// `scoring_year` (the year of the day being scored).
pub fn net_worth_inputs(
    account: AccountId,
    ledger: &Ledger,
    map: &Map,
    businesses: &Businesses,
    goods: &BTreeMap<String, Good>,
    scoring_year: i32,
) -> NetWorthInputs {
    let mut inputs = NetWorthInputs {
        cash: ledger.balance(account),
        ..NetWorthInputs::default()
    };
    for business in businesses.by_id.values() {
        if business.owner == account {
            inputs.stock += stock_value(&business.stock, goods);
            inputs.owned_property_values.push(business.build_cost);
        }
    }
    for (index, block) in map.blocks.iter().enumerate() {
        let Some(lease) = block.lease.as_ref().filter(|l| l.holder == account) else {
            continue;
        };
        let Ok(index) = u32::try_from(index) else {
            continue;
        };
        inputs.held_lots.push(HeldLot {
            land_value: land_value(
                lease.last_price,
                lease.remaining_years(scoring_year),
                lease.term_years,
            ),
            property_values: businesses
                .by_id
                .values()
                .filter(|b| b.block.0 == index)
                .map(|b| b.build_cost)
                .collect(),
        });
    }
    inputs
}

/// Every player's net worth as of the last midnight: the nightly leaderboard. After the
/// final midnight it is the final score (R19, R31).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct NetWorthBoard {
    latest: BTreeMap<PlayerId, Money>,
}

impl NetWorthBoard {
    /// Records a player's net worth for tonight.
    pub fn record(&mut self, player: PlayerId, value: Money) {
        self.latest.insert(player, value);
    }

    /// A player's net worth at the last midnight, if they have been scored.
    pub fn get(&self, player: PlayerId) -> Option<Money> {
        self.latest.get(&player).copied()
    }

    /// Players from richest to poorest; ties go to the lower seat.
    pub fn ranking(&self) -> Vec<(PlayerId, Money)> {
        let mut rows: Vec<(PlayerId, Money)> = self.latest.iter().map(|(p, m)| (*p, *m)).collect();
        rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn owning_land_and_building_earns_the_uplift() {
        let inputs = NetWorthInputs {
            cash: Money::from_pounds(100),
            stock: Money::from_pounds(10),
            owned_property_values: vec![Money::from_pounds(1_000)],
            held_lots: vec![HeldLot {
                land_value: Money::from_pounds(500),
                property_values: vec![Money::from_pounds(1_000)],
            }],
        };
        // 100 + 10 + 1000 + 500 + 0.2 × 1000
        assert_eq!(
            net_worth(&inputs, Rate::from_bp(2_000)),
            Money::from_pounds(1_810)
        );
    }

    #[test]
    fn land_value_scales_with_years_left_and_rounds_to_the_penny() {
        let price = Money::from_pounds(5_500);
        assert_eq!(land_value(price, 30, 30), price);
        // 550,000p × 25 / 30 = 458,333.33p
        assert_eq!(land_value(price, 25, 30), Money::from_pence(458_333));
        // 100p × 1 / 3 = 33.33p; 200p × 1 / 3 = 66.67p (rounds up)
        assert_eq!(
            land_value(Money::from_pence(100), 1, 3),
            Money::from_pence(33)
        );
        assert_eq!(
            land_value(Money::from_pence(200), 1, 3),
            Money::from_pence(67)
        );
        assert_eq!(land_value(price, 0, 30), Money::ZERO);
        assert_eq!(land_value(price, 5, 0), Money::ZERO);
    }

    #[test]
    fn stock_is_valued_at_purchase_price_and_ignores_unknown_goods() {
        let goods: BTreeMap<String, Good> = [(
            "bread".to_owned(),
            Good {
                name: "Bread".into(),
                ref_price: crate::config::types::Pounds(Money::from_pence(10)),
                purchase_price: crate::config::types::Pounds(Money::from_pence(4)),
                luxury: false,
            },
        )]
        .into();
        let stock: BTreeMap<String, i64> =
            [("bread".to_owned(), 25), ("caviar".to_owned(), 9)].into();
        assert_eq!(stock_value(&stock, &goods), Money::from_pence(100));
    }

    #[test]
    fn ranking_is_richest_first_with_ties_to_the_lower_seat() {
        let mut board = NetWorthBoard::default();
        board.record(PlayerId(0), Money::from_pounds(5));
        board.record(PlayerId(1), Money::from_pounds(9));
        board.record(PlayerId(2), Money::from_pounds(9));
        let order: Vec<PlayerId> = board.ranking().into_iter().map(|r| r.0).collect();
        assert_eq!(order, vec![PlayerId(1), PlayerId(2), PlayerId(0)]);
    }

    proptest! {
        #[test]
        fn land_value_never_exceeds_the_price_paid(
            pence in 0i64..10_000_000_000,
            term in 1u32..200,
            left in 0u32..200,
        ) {
            let left = left.min(term);
            let v = land_value(Money::from_pence(pence), left, term);
            prop_assert!(v >= Money::ZERO);
            prop_assert!(v <= Money::from_pence(pence));
        }

        #[test]
        fn more_property_on_your_lot_never_lowers_net_worth(
            base in prop::collection::vec(0i64..1_000_000, 0..10),
            extra in 0i64..1_000_000,
        ) {
            let lot = |vals: Vec<Money>| NetWorthInputs {
                held_lots: vec![HeldLot { land_value: Money::ZERO, property_values: vals }],
                ..NetWorthInputs::default()
            };
            let before: Vec<Money> = base.iter().copied().map(Money::from_pence).collect();
            let mut after = before.clone();
            after.push(Money::from_pence(extra));
            let uplift = Rate::from_bp(2_000);
            prop_assert!(net_worth(&lot(after), uplift) >= net_worth(&lot(before), uplift));
        }
    }
}
