//! A shopper's list as errands with windows (R70): the order they are worked in, and what the
//! shopper does next at any moment: go for the first item, wait for its window or for the
//! lights (R69), or drop items whose window has passed. A dropped item is missed, not a lost
//! sale against any shop.

use std::collections::BTreeMap;

use super::shops::{GoodId, ShopOffer};
use super::streets::Streets;
use super::windows::Span;
use super::{Activity, Customer, Errand, choose_shop};
use crate::businesses::BusinessId;
use crate::config::Config;
use crate::money::Rate;
use crate::rng::{SimRng, below};

/// A rolled list as errands, each with its window from `table` (by good id; `whole` if
/// missing), in window order: earliest opening first, then earliest closing, then good id
/// (R70).
pub fn errands(goods: &[GoodId], table: &[Span], whole: Span) -> Vec<Errand> {
    let mut list: Vec<Errand> = goods
        .iter()
        .map(|g| {
            let span = table.get(usize::from(g.0)).copied().unwrap_or(whole);
            Errand {
                good: *g,
                from: span.from,
                to: span.to,
            }
        })
        .collect();
    list.sort_by_key(|e| (e.from, e.to, e.good));
    list
}

/// When a shopper sets out (R70): at their earliest window's opening, trickling in over the
/// spawn spread (one draw from `rng`) but never so late that more than half that window has
/// gone. An empty list sets out as `whole` (the trade) opens.
pub fn set_out(
    list: &[Errand],
    whole: Span,
    spread_ticks: u64,
    tick_secs: u32,
    rng: &mut SimRng,
) -> u32 {
    let first = list.first().map_or(whole, |e| Span {
        from: e.from,
        to: e.to,
    });
    let half_window = u64::from(first.to.saturating_sub(first.from) / 2 / tick_secs.max(1));
    let delay = u32::try_from(below(rng, spread_ticks.min(half_window) + 1)).unwrap_or(0);
    first.from + delay * tick_secs
}

/// Where a shopper stands with their list at `now` (R70), given when the lights come back on
/// if a power cut has the shops dark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Plan {
    /// The first item's window is open and the lights are on: choose a shop for it.
    Go,
    /// Wait until this second: for the first item's window to open (`power` false), or for the
    /// lights, inside that window (`power` true).
    Wait {
        /// When to look again.
        until: u32,
        /// The lights, not the window, are what they are waiting for.
        power: bool,
    },
    /// Nothing left on the list.
    Done,
}

/// Drops the items whose window has passed, or will have passed before the lights come back on
/// (R70: missed, not bought today; not a lost sale), and says what the shopper does next.
/// Returns the plan and how many items were dropped.
pub(crate) fn plan(c: &mut Customer, now: u32, dark_until: Option<u32>) -> (Plan, u64) {
    let mut missed = 0;
    while let Some(e) = c.list.first().copied() {
        let lit_at = dark_until.unwrap_or(now);
        if e.to <= now || e.to <= lit_at {
            c.list.remove(0);
            missed += 1;
            continue;
        }
        let ready = e.from.max(lit_at);
        if ready <= now {
            return (Plan::Go, missed);
        }
        let power = dark_until.is_some_and(|u| u >= e.from);
        return (
            Plan::Wait {
                until: ready,
                power,
            },
            missed,
        );
    }
    (Plan::Done, missed)
}

/// The shopper's next move (R70): walk to a shop for the first item on their list whose window
/// is open, dropping items whose window has passed or that no shop can sell them now; wait for
/// the next window (off the map) or for the lights (on the street); or `Leaving` once the list
/// is empty. Returns the activity and how many items were missed.
#[allow(clippy::too_many_arguments, reason = "the §8.3 inputs plus the time")]
pub(crate) fn next_errand(
    c: &mut Customer,
    now: u32,
    dark_until: Option<u32>,
    offers: &[ShopOffer],
    reputation: &BTreeMap<BusinessId, Rate>,
    config: &Config,
    streets: &Streets,
    rng: &mut SimRng,
) -> (Activity, u64) {
    let mut missed = 0;
    loop {
        let (next, dropped) = plan(c, now, dark_until);
        missed += dropped;
        let good = match next {
            Plan::Done => return (Activity::Leaving, missed),
            Plan::Wait { until, power: true } => {
                return (Activity::WaitingForPower { until }, missed);
            }
            Plan::Wait { until, .. } => return (Activity::Away { until }, missed),
            Plan::Go => c.list[0].good,
        };
        if let Some(i) = choose_shop(
            c.cohort, c.at, c.budget, good, offers, reputation, config, streets, rng,
        ) {
            let to = offers[i].door;
            return (
                Activity::Walking {
                    to,
                    shop: offers[i].shop,
                    good,
                },
                missed,
            );
        }
        // No open shop has it in stock for them: nothing will change before the window closes
        // (no mid-day restock, R2), so the window is missed.
        c.list.remove(0);
        missed += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::customers::movement::Cell;
    use crate::customers::{CustomerId, Origin, Wave};
    use crate::money::Money;
    use crate::population::{AgeBand, Class, Cohort, Sex};

    const H: u32 = 3600;

    fn errand(good: u16, from: u32, to: u32) -> Errand {
        Errand {
            good: GoodId(good),
            from,
            to,
        }
    }

    fn shopper(list: Vec<Errand>) -> Customer {
        Customer {
            id: CustomerId(1),
            cohort: Cohort {
                band: AgeBand(1),
                sex: Sex::Male,
                class: Class::Working,
            },
            origin: Origin::Commuter,
            wave: Wave::Day,
            at: Cell { x: 0, y: 0 },
            exit: Cell { x: 0, y: 0 },
            budget: Money::from_pounds(10),
            list,
            activity: Activity::Choosing,
        }
    }

    #[test]
    fn errands_run_in_window_order() {
        let whole = Span {
            from: 7 * H,
            to: 17 * H,
        };
        let mut table = vec![whole; 3];
        table[0] = Span {
            from: 12 * H,
            to: 13 * H,
        };
        table[2] = Span {
            from: 7 * H,
            to: 8 * H,
        };
        let list = errands(&[GoodId(0), GoodId(1), GoodId(2)], &table, whole);
        let order: Vec<u16> = list.iter().map(|e| e.good.0).collect();
        // 07:00–08:00 first; 07:00–17:00 next (same opening, closes later); noon last.
        assert_eq!(order, vec![2, 1, 0]);
        // A good missing from the table gets the trade's whole hours.
        assert_eq!(errands(&[GoodId(9)], &table, whole)[0].from, 7 * H);
    }

    #[test]
    fn inside_an_open_window_with_the_lights_on_they_go() {
        let mut c = shopper(vec![errand(0, 8 * H, 9 * H)]);
        assert_eq!(plan(&mut c, 8 * H, None), (Plan::Go, 0));
    }

    #[test]
    fn before_the_window_they_wait_for_it() {
        let mut c = shopper(vec![errand(0, 12 * H, 13 * H)]);
        let wait = Plan::Wait {
            until: 12 * H,
            power: false,
        };
        assert_eq!(plan(&mut c, 9 * H, None), (wait, 0));
    }

    #[test]
    fn a_passed_window_is_dropped_and_the_next_item_planned() {
        let mut c = shopper(vec![errand(0, 8 * H, 9 * H), errand(1, 10 * H, 11 * H)]);
        let wait = Plan::Wait {
            until: 10 * H,
            power: false,
        };
        assert_eq!(
            plan(&mut c, 9 * H, None),
            (wait, 1),
            "09:00 is past 08:00–09:00"
        );
        assert_eq!(c.list.len(), 1);
        assert_eq!(c.list[0].good, GoodId(1));
    }

    #[test]
    fn a_cut_that_outlasts_the_window_drops_the_item() {
        let mut c = shopper(vec![errand(0, 8 * H, 9 * H), errand(1, 10 * H, 11 * H)]);
        // Dark from 08:00 until 09:30: the bread window closes in the dark.
        let wait = Plan::Wait {
            until: 10 * H,
            power: false,
        };
        assert_eq!(plan(&mut c, 8 * H, Some(9 * H + 1800)), (wait, 1));
        // A cut that ends exactly as the window closes leaves no time to buy either.
        let mut c = shopper(vec![errand(0, 8 * H, 9 * H)]);
        assert_eq!(plan(&mut c, 8 * H, Some(9 * H)), (Plan::Done, 1));
    }

    #[test]
    fn a_cut_that_ends_inside_the_window_is_waited_out() {
        let mut c = shopper(vec![errand(0, 8 * H, 9 * H)]);
        let wait = Plan::Wait {
            until: 8 * H + 1800,
            power: true,
        };
        assert_eq!(plan(&mut c, 8 * H, Some(8 * H + 1800)), (wait, 0));
        assert_eq!(c.list.len(), 1);
    }

    #[test]
    fn a_cut_before_a_later_window_is_not_waited_for() {
        // Dark until 09:00, the window opens at noon: wait for the window, not the lights.
        let mut c = shopper(vec![errand(0, 12 * H, 13 * H)]);
        let wait = Plan::Wait {
            until: 12 * H,
            power: false,
        };
        assert_eq!(plan(&mut c, 8 * H, Some(9 * H)), (wait, 0));
    }

    #[test]
    fn an_empty_list_is_done() {
        let mut c = shopper(Vec::new());
        assert_eq!(plan(&mut c, 8 * H, None), (Plan::Done, 0));
    }
}
