//! Shopping windows (R70): when in the day each good is shopped for, by age band, class, sex,
//! trade and year, from `customers.yaml` `shopping_windows`.
//!
//! Rules apply in order, like the desire table ([`super::shopping`]): every rule whose match fits
//! the shopper sets the window of each good it names, so a later, narrower rule overrides an
//! earlier, broader one. Within one rule `"*"` (every good) applies first, then the groups it
//! names (in name order), then single goods. Times are seconds of the day; a window runs from
//! `from` (inclusive) to `to` (exclusive). Integers only.

use std::collections::BTreeMap;

use super::shops::{GoodId, GoodsIndex};
use crate::businesses::Hours;
use crate::config::CustomersConfig;
use crate::config::customers::{EVERY_GOOD, ShopWindow, WindowMatch};
use crate::population::{Cohort, band_id};

/// A window as seconds of the day: `from` (inclusive) to `to` (exclusive).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Span {
    /// Opens.
    pub from: u32,
    /// Closes.
    pub to: u32,
}

impl From<ShopWindow> for Span {
    fn from(w: ShopWindow) -> Self {
        Span {
            from: w.from.seconds_since_midnight(),
            to: w.to.seconds_since_midnight(),
        }
    }
}

/// True if a window rule's match covers this shopper, trade and year.
pub fn window_rule_matches(
    m: &WindowMatch,
    rules: &CustomersConfig,
    cohort: Cohort,
    trade: Hours,
    year: i32,
) -> bool {
    m.trade == trade
        && m.class.is_none_or(|c| c == cohort.class)
        && m.sex.is_none_or(|s| s == cohort.sex)
        && m.age
            .as_deref()
            .is_none_or(|a| a == band_id(rules, cohort.band))
        && m.from_year.is_none_or(|y| year >= y)
        && m.until_year.is_none_or(|y| year <= y)
}

/// Every window one cohort's matching rules give, for one trade and year: the `"*"` window
/// (if any) and the windows set for named goods on top of it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Resolved<'a> {
    every: Option<ShopWindow>,
    named: BTreeMap<&'a str, ShopWindow>,
}

impl Resolved<'_> {
    /// The window for `good`, if any rule covers it.
    pub fn get(&self, good: &str) -> Option<Span> {
        self.named
            .get(good)
            .or(self.every.as_ref())
            .map(|w| Span::from(*w))
    }
}

/// Applies the matching rules in order (R70): a rule's `"*"` replaces everything set so far,
/// then its groups (in name order) and its single goods set their members' windows.
pub fn resolve(rules: &CustomersConfig, cohort: Cohort, trade: Hours, year: i32) -> Resolved<'_> {
    let groups = &rules.shopping_windows.groups;
    let mut out = Resolved::default();
    for rule in &rules.shopping_windows.rules {
        if !window_rule_matches(&rule.matcher, rules, cohort, trade, year) {
            continue;
        }
        if let Some(w) = rule.windows.get(EVERY_GOOD) {
            out.every = Some(*w);
            out.named.clear();
        }
        for (name, w) in &rule.windows {
            for good in groups.get(name).into_iter().flatten() {
                out.named.insert(good.as_str(), *w);
            }
        }
        for (name, w) in &rule.windows {
            if name != EVERY_GOOD && !groups.contains_key(name) {
                out.named.insert(name.as_str(), *w);
            }
        }
    }
    out
}

/// The window in which this shopper shops for `good` in `trade` and `year` (R70); `None` if no
/// rule covers it (config validation forbids that for any good the shopper can want).
pub fn shopping_window(
    rules: &CustomersConfig,
    cohort: Cohort,
    trade: Hours,
    year: i32,
    good: &str,
) -> Option<Span> {
    resolve(rules, cohort, trade, year).get(good)
}

/// Every good's window for one cohort, trade and year, by [`GoodId`] (resolved once per cohort
/// as a wave spawns). A good no rule covers gets `fallback`, the trade's whole opening hours.
pub fn window_table(
    rules: &CustomersConfig,
    goods: &GoodsIndex,
    cohort: Cohort,
    trade: Hours,
    year: i32,
    fallback: Span,
) -> Vec<Span> {
    let resolved = resolve(rules, cohort, trade, year);
    (0..goods.len())
        .map(|i| {
            let id = GoodId(u16::try_from(i).unwrap_or(u16::MAX));
            resolved.get(goods.name(id)).unwrap_or(fallback)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::customers::WindowRule;
    use crate::config::{ClockTime, Config, ConfigFile, ConfigSources};
    use crate::population::{AgeBand, Class, Sex};

    fn hm(text: &str) -> ClockTime {
        ClockTime::parse(text).unwrap_or_default()
    }

    fn window(from: &str, to: &str) -> ShopWindow {
        ShopWindow {
            from: hm(from),
            to: hm(to),
        }
    }

    fn rule(trade: Hours, class: Option<Class>, windows: &[(&str, ShopWindow)]) -> WindowRule {
        WindowRule {
            matcher: WindowMatch {
                trade,
                class,
                age: None,
                sex: None,
                from_year: None,
                until_year: None,
            },
            windows: windows.iter().map(|(k, w)| ((*k).to_owned(), *w)).collect(),
        }
    }

    fn rules() -> CustomersConfig {
        let text = |f: ConfigFile| match f {
            ConfigFile::Game => include_str!("../../../../config/game.yaml"),
            ConfigFile::Map => include_str!("../../../../config/map.yaml"),
            ConfigFile::Auctions => include_str!("../../../../config/auctions.yaml"),
            ConfigFile::Utilities => include_str!("../../../../config/utilities.yaml"),
            ConfigFile::Businesses => include_str!("../../../../config/businesses.yaml"),
            ConfigFile::Customers => include_str!("../../../../config/customers.yaml"),
            ConfigFile::Events => include_str!("../../../../config/events.yaml"),
            ConfigFile::Ai => include_str!("../../../../config/ai.yaml"),
            ConfigFile::Economy => include_str!("../../../../config/economy.yaml"),
        };
        let mut sources = ConfigSources::new();
        for f in ConfigFile::ALL {
            sources.insert(f.file_name(), text(f));
        }
        let mut c = Config::load(&sources).map_or_else(|e| panic!("{e}"), |c| c.customers);
        c.shopping_windows.groups = [("food".to_owned(), vec!["bread".to_owned()])].into();
        c.shopping_windows.rules = vec![
            rule(Hours::Day, None, &[("*", window("07:00", "17:00"))]),
            rule(
                Hours::Day,
                Some(Class::Working),
                &[
                    ("*", window("16:00", "17:00")),
                    ("food", window("12:00", "14:00")),
                    ("bread", window("12:30", "13:00")),
                ],
            ),
            rule(
                Hours::Day,
                Some(Class::Upper),
                &[("food", window("10:00", "11:00"))],
            ),
        ];
        c
    }

    fn cohort(class: Class) -> Cohort {
        Cohort {
            band: AgeBand(1),
            sex: Sex::Female,
            class,
        }
    }

    fn at(r: &CustomersConfig, class: Class, good: &str) -> Option<(u32, u32)> {
        shopping_window(r, cohort(class), Hours::Day, 1930, good).map(|s| (s.from, s.to))
    }

    #[test]
    fn later_narrower_rules_override_and_goods_beat_groups_beat_every_good() {
        let r = rules();
        assert_eq!(at(&r, Class::Middle, "bread"), Some((7 * 3600, 17 * 3600)));
        assert_eq!(
            at(&r, Class::Working, "bread"),
            Some((12 * 3600 + 1800, 13 * 3600))
        );
        assert_eq!(
            at(&r, Class::Working, "groceries"),
            Some((16 * 3600, 17 * 3600))
        );
        // The upper-class rule names only the food group: bread from it, the rest from "*".
        assert_eq!(at(&r, Class::Upper, "bread"), Some((10 * 3600, 11 * 3600)));
        assert_eq!(at(&r, Class::Upper, "clothes"), Some((7 * 3600, 17 * 3600)));
    }

    #[test]
    fn rules_apply_to_their_trade_only() {
        let r = rules();
        assert_eq!(
            shopping_window(&r, cohort(Class::Middle), Hours::Night, 1930, "bread"),
            None
        );
    }
}
