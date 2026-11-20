//! The modifier stack (§9): every lasting effect of an event, in one place, for other modules
//! to query.
//!
//! An event that lasts (rationing, a boom, the three-day week, Decimal Day's price confusion,
//! the Olympics) pushes a typed [`Modifier`] here with the game day it ends. Other modules never
//! special-case an event: they ask the stack.
//!
//! | Query | Who asks | Used for |
//! |---|---|---|
//! | [`Modifiers::demand_factor`] | customers, when a shopper rolls their list | demand by good and class |
//! | [`Modifiers::supply_cap`] | the world, after the midnight order lock | rationing caps |
//! | [`Modifiers::may_trade`] | customers, when listing open shops | open days (three-day week) |
//! | [`Modifiers::price_factor`] | customers, on the shelf price and at the till | price noise |
//! | [`Modifiers::footfall`] | customers, on a shop's pull | footfall by block |
//!
//! Stacking: demand, price and footfall factors multiply; supply caps take the tightest; open
//! days must all allow the shop. An empty stack answers ×1 / no cap / open everywhere, and
//! [`Modifiers::is_empty`] lets hot paths skip the queries.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::businesses::BusinessId;
use crate::land::BlockId;
use crate::money::{BASIS_POINTS_PER_UNIT, Rate};
use crate::population::Class;

/// One lasting effect.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Modifier {
    /// Demand for `goods` (every good if `None`) by shoppers of `class` (every class if
    /// `None`) is multiplied by `factor`.
    Demand {
        /// Goods affected, or all.
        goods: Option<BTreeSet<String>>,
        /// Class affected, or all.
        class: Option<Class>,
        /// Multiplier.
        factor: Rate,
    },
    /// A shop may stock at most `share` of its capacity of each of `goods`.
    SupplyCap {
        /// Goods capped.
        goods: BTreeSet<String>,
        /// Share of capacity.
        share: Rate,
    },
    /// Shops trade `open` days in `of`: on game day `d`, shop `id` trades iff
    /// `(id + d) mod of < open`.
    OpenDays {
        /// Days open.
        open: u32,
        /// Out of.
        of: u32,
    },
    /// Per shop and good, the shelf price is multiplied by a factor drawn when the event fired.
    PriceNoise {
        /// Factor per (shop, good).
        factors: BTreeMap<(BusinessId, String), Rate>,
    },
    /// Shops on `blocks` pull `factor` × the shoppers.
    Footfall {
        /// Blocks.
        blocks: BTreeSet<BlockId>,
        /// Multiplier.
        factor: Rate,
    },
}

/// A modifier on the stack, with where it came from and when it ends.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ActiveModifier {
    /// The event id that pushed it.
    pub source: String,
    /// The first game day it no longer applies (it is dropped at that day's midnight).
    pub until_day: u32,
    /// The effect.
    pub modifier: Modifier,
}

/// The stack.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Modifiers {
    active: Vec<ActiveModifier>,
}

/// `a × b` in basis points, rounding half away from zero.
pub fn compose(a: Rate, b: Rate) -> Rate {
    let product = i128::from(a.bp()) * i128::from(b.bp());
    let denom = i128::from(BASIS_POINTS_PER_UNIT);
    let rounded = if product >= 0 {
        (product + denom / 2) / denom
    } else {
        (product - denom / 2) / denom
    };
    Rate::from_bp(i64::try_from(rounded).unwrap_or(i64::MAX))
}

impl Modifiers {
    /// True if nothing is in force (every query is neutral).
    pub fn is_empty(&self) -> bool {
        self.active.is_empty()
    }

    /// Everything in force, oldest first.
    pub fn active(&self) -> &[ActiveModifier] {
        &self.active
    }

    /// Pushes a modifier lasting until `until_day` (exclusive).
    pub fn push(&mut self, source: impl Into<String>, until_day: u32, modifier: Modifier) {
        self.active.push(ActiveModifier {
            source: source.into(),
            until_day,
            modifier,
        });
    }

    /// Drops everything that has run out by game day `day`.
    pub fn expire(&mut self, day: u32) {
        self.active.retain(|m| m.until_day > day);
    }

    /// Demand multiplier for `good` bought by a shopper of `class` (×1 with nothing in force).
    pub fn demand_factor(&self, good: &str, class: Class) -> Rate {
        self.active
            .iter()
            .filter_map(|m| match &m.modifier {
                Modifier::Demand {
                    goods,
                    class: only,
                    factor,
                } if goods.as_ref().is_none_or(|g| g.contains(good))
                    && only.is_none_or(|c| c == class) =>
                {
                    Some(*factor)
                }
                _ => None,
            })
            .fold(Rate::ONE, compose)
    }

    /// The tightest supply cap on `good`, as a share of a shop's capacity, if any.
    pub fn supply_cap(&self, good: &str) -> Option<Rate> {
        self.active
            .iter()
            .filter_map(|m| match &m.modifier {
                Modifier::SupplyCap { goods, share } if goods.contains(good) => Some(*share),
                _ => None,
            })
            .min()
    }

    /// True if shop `shop` may trade on game day `day` under every open-days rule in force.
    pub fn may_trade(&self, shop: BusinessId, day: u32) -> bool {
        self.active.iter().all(|m| match m.modifier {
            Modifier::OpenDays { open, of } if of > 0 => {
                (u64::from(shop.0) + u64::from(day)) % u64::from(of) < u64::from(open)
            }
            _ => true,
        })
    }

    /// Multiplier on `shop`'s shelf price of `good` (×1 with no price noise in force).
    pub fn price_factor(&self, shop: BusinessId, good: &str) -> Rate {
        self.active
            .iter()
            .filter_map(|m| match &m.modifier {
                Modifier::PriceNoise { factors } => factors.get(&(shop, good.to_owned())).copied(),
                _ => None,
            })
            .fold(Rate::ONE, compose)
    }

    /// Multiplier on the pull of shops standing on `block`.
    pub fn footfall(&self, block: BlockId) -> Rate {
        self.active
            .iter()
            .filter_map(|m| match &m.modifier {
                Modifier::Footfall { blocks, factor } if blocks.contains(&block) => Some(*factor),
                _ => None,
            })
            .fold(Rate::ONE, compose)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn goods(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn an_empty_stack_is_neutral() {
        let m = Modifiers::default();
        assert!(m.is_empty());
        assert_eq!(m.demand_factor("bread", Class::Upper), Rate::ONE);
        assert_eq!(m.supply_cap("bread"), None);
        assert!(m.may_trade(BusinessId(3), 9));
        assert_eq!(m.price_factor(BusinessId(3), "bread"), Rate::ONE);
        assert_eq!(m.footfall(BlockId(0)), Rate::ONE);
    }

    #[test]
    fn demand_factors_multiply_and_match_by_good_and_class() {
        let mut m = Modifiers::default();
        m.push(
            "a",
            5,
            Modifier::Demand {
                goods: Some(goods(&["clothes"])),
                class: None,
                factor: Rate::from_bp(4_000),
            },
        );
        m.push(
            "b",
            5,
            Modifier::Demand {
                goods: None,
                class: Some(Class::Upper),
                factor: Rate::from_bp(8_000),
            },
        );
        assert_eq!(
            m.demand_factor("clothes", Class::Upper),
            Rate::from_bp(3_200)
        );
        assert_eq!(
            m.demand_factor("clothes", Class::Working),
            Rate::from_bp(4_000)
        );
        assert_eq!(m.demand_factor("bread", Class::Upper), Rate::from_bp(8_000));
        assert_eq!(m.demand_factor("bread", Class::Middle), Rate::ONE);
    }

    #[test]
    fn modifiers_expire_on_their_day() {
        let mut m = Modifiers::default();
        m.push(
            "cap",
            3,
            Modifier::SupplyCap {
                goods: goods(&["bread"]),
                share: Rate::from_bp(5_000),
            },
        );
        m.expire(2);
        assert_eq!(m.supply_cap("bread"), Some(Rate::from_bp(5_000)));
        m.expire(3);
        assert_eq!(m.supply_cap("bread"), None);
    }

    #[test]
    fn three_days_in_seven_rotates_shops() {
        let mut m = Modifiers::default();
        m.push("week", 10, Modifier::OpenDays { open: 3, of: 7 });
        for day in 0..7 {
            let open = (0..7).filter(|s| m.may_trade(BusinessId(*s), day)).count();
            assert_eq!(open, 3, "day {day}");
        }
        let days_open = (0..7).filter(|d| m.may_trade(BusinessId(4), *d)).count();
        assert_eq!(days_open, 3);
    }

    #[test]
    fn the_tightest_cap_wins() {
        let mut m = Modifiers::default();
        for share in [5_000, 2_500] {
            m.push(
                "cap",
                3,
                Modifier::SupplyCap {
                    goods: goods(&["bread"]),
                    share: Rate::from_bp(share),
                },
            );
        }
        assert_eq!(m.supply_cap("bread"), Some(Rate::from_bp(2_500)));
    }
}
