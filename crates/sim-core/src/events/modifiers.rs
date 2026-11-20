//! The modifier stack (§9): every lasting effect of an event, in one place, for other modules
//! to query.
//!
//! An event that lasts (rationing, a boom, the 1970s power cuts, Decimal Day's price confusion,
//! the Olympics) pushes a typed [`Modifier`] here with the game day it ends. Other modules never
//! special-case an event: they ask the stack.
//!
//! | Query | Who asks | Used for |
//! |---|---|---|
//! | [`Modifiers::demand_factor`] | customers, when a shopper rolls their list | demand by good and class |
//! | [`Modifiers::supply_cap`] | the world, after the midnight order lock | rationing caps |
//! | [`Modifiers::dark_until`] | customers, each tick | power cuts (R69) |
//! | [`Modifiers::price_factor`] | customers, on the shelf price and at the till | price noise |
//! | [`Modifiers::footfall`] | customers, on a shop's pull | footfall by block |
//!
//! Stacking: demand, price and footfall factors multiply; supply caps take the tightest; the
//! shops are dark while any power cut's window covers the time. An empty stack answers ×1 / no
//! cap / lights on, and
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
    /// Power cuts (R69): every walk-in shop is dark from `from` (inclusive) to `to`
    /// (exclusive) in each window, as seconds of the day.
    PowerCut {
        /// `(from, to)` seconds of day.
        windows: Vec<(u32, u32)>,
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

    /// If a power cut has the shops dark at `second` of the day, the second the lights come
    /// back on (windows that meet are one stretch of dark); `None` with the lights on.
    pub fn dark_until(&self, second: u32) -> Option<u32> {
        let windows = || {
            self.active.iter().flat_map(|m| match &m.modifier {
                Modifier::PowerCut { windows } => windows.as_slice(),
                _ => &[],
            })
        };
        let mut until = second;
        while let Some((_, to)) = windows().find(|(from, to)| *from <= until && until < *to) {
            until = *to;
        }
        (until > second).then_some(until)
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
        assert_eq!(m.dark_until(8 * 3600), None);
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
    fn power_cuts_are_dark_inside_their_windows_only() {
        let h = |hours: u32, minutes: u32| hours * 3600 + minutes * 60;
        let mut m = Modifiers::default();
        m.push(
            "cuts",
            1,
            Modifier::PowerCut {
                windows: vec![(h(7, 0), h(10, 0)), (h(13, 0), h(15, 30))],
            },
        );
        assert_eq!(m.dark_until(h(6, 59)), None);
        assert_eq!(m.dark_until(h(7, 0)), Some(h(10, 0)));
        assert_eq!(m.dark_until(h(9, 59)), Some(h(10, 0)));
        assert_eq!(m.dark_until(h(10, 0)), None, "the lights are back at 10:00");
        assert_eq!(m.dark_until(h(14, 0)), Some(h(15, 30)));
        assert_eq!(m.dark_until(h(15, 30)), None);
        assert_eq!(m.dark_until(h(20, 0)), None, "night trade is lit");
        // A second cut that meets the first makes one longer stretch of dark.
        m.push(
            "more",
            1,
            Modifier::PowerCut {
                windows: vec![(h(10, 0), h(11, 0))],
            },
        );
        assert_eq!(m.dark_until(h(8, 0)), Some(h(11, 0)));
        m.expire(1);
        assert_eq!(
            m.dark_until(h(8, 0)),
            None,
            "the year after, the lights stay on"
        );
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
