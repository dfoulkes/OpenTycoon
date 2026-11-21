//! Opportunism (R66): what each line of business is earning, read from public state, and when a
//! personality should leave its home line for another.
//!
//! A line's **yield** is its daily net earnings per pound tied up in it:
//!
//! - **residential**: for every apartment block, `flats let × rent` (R51) less its land rent and
//!   utilities (R56), over its build cost (R41);
//! - **retail**: for every walk-in shop, the day's takings less its running cost (land rent,
//!   utilities and the standing order at wholesale, §7.1), over its build cost;
//! - **land**: for every leased lot, the land rent other owners' buildings pay the holder (§5.3),
//!   over what the holder paid for the lease.
//!
//! The AI reads the yields once a day, as soon as night trade has closed and the day's takings
//! are complete ([`Outlook::observe`]); it never scans the map tick by tick. It compares the
//! market's yield on the other line with its own on its home line (the market's, if it has
//! nothing in its home line yet) and moves when the other is better by `threshold`
//! ([`beats`]), never letting more than `max_outside_share` of its holdings sit outside its home
//! line ([`within_cap`]).

use serde::{Deserialize, Serialize};

use super::rent::{RentBook, VacancyRule};
use super::{Plan, apartments_type, owned_on, plan_on, spare};
use crate::businesses::{Business, Hours};
use crate::config::Config;
use crate::config::ai::OpportunismConfig;
use crate::config::businesses::Channel;
use crate::economy::{AccountId, PlayerId};
use crate::land::{BlockId, land_rent_per_day};
use crate::money::{BASIS_POINTS_PER_UNIT, Money, Rate};
use crate::population::residential::flats;
use crate::world::{Command, World};

/// A line of business (R66).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Line {
    /// Walk-in shops (§7.1).
    Retail,
    /// Apartment blocks (§7.2).
    Residential,
    /// Leases, earning land rent (§5.2).
    Land,
}

/// The line a building is in: apartments are residential, walk-in day and night shops are
/// retail; anything else (Hotels, online, infrastructure) is in no line.
pub fn line_of(config: &Config, b: &Business) -> Option<Line> {
    if b.type_id == config.customers.residential.building_type {
        return Some(Line::Residential);
    }
    let kind = config.businesses.types.get(&b.type_id)?;
    (kind.channel == Channel::Shop
        && !kind.goods.is_empty()
        && matches!(kind.hours, Hours::Day | Hours::Night))
    .then_some(Line::Retail)
}

/// `net ÷ capital` as a daily rate, in basis points. `None` with nothing tied up.
pub fn daily_yield(net: Money, capital: Money) -> Option<Rate> {
    if capital.pence() <= 0 {
        return None;
    }
    let bp =
        i128::from(net.pence()) * i128::from(BASIS_POINTS_PER_UNIT) / i128::from(capital.pence());
    Some(Rate::from_bp(i64::try_from(bp).unwrap_or(if bp < 0 {
        i64::MIN
    } else {
        i64::MAX
    })))
}

/// Each line's yield. `None` for a line with nothing in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct LineYields {
    /// Walk-in shops.
    pub retail: Option<Rate>,
    /// Apartment blocks.
    pub residential: Option<Rate>,
    /// Leases.
    pub land: Option<Rate>,
}

impl LineYields {
    /// The yield of `line`.
    pub fn of(&self, line: Line) -> Option<Rate> {
        match line {
            Line::Retail => self.retail,
            Line::Residential => self.residential,
            Line::Land => self.land,
        }
    }
}

/// What every line is earning now, over the whole city or (with `owner`) over one account's
/// buildings and leases only. Shops' takings are the day's so far, so read it after night trade
/// closes ([`Outlook`]).
pub fn line_yields(world: &World, owner: Option<AccountId>) -> LineYields {
    let config = world.config();
    let rules = &config.economy.residential;
    let year = world.clock().year();
    let occupancy = world.population().residential.occupancy();
    let mine = |account: AccountId| owner.is_none_or(|o| o == account);
    let (mut retail, mut residential) = ((Money::ZERO, Money::ZERO), (Money::ZERO, Money::ZERO));
    for (id, b) in &world.businesses().by_id {
        if !mine(b.owner) {
            continue;
        }
        let Some(line) = line_of(config, b) else {
            continue;
        };
        let Some(cost) = world
            .businesses()
            .running_cost(*id, config, world.map(), year)
        else {
            continue;
        };
        let (income, sum) = match line {
            Line::Residential => {
                let let_flats = occupancy
                    .get(id)
                    .copied()
                    .unwrap_or(0)
                    .min(flats(b.volume(), rules.flat_size));
                let rent = world
                    .population()
                    .residential
                    .rent(*id, rules.default_rent.money());
                (rent.saturating_mul(i64::from(let_flats)), &mut residential)
            }
            Line::Retail | Line::Land => (b.takings_today, &mut retail),
        };
        sum.0 += income - cost.total();
        sum.1 += b.build_cost;
    }
    let mut land = (Money::ZERO, Money::ZERO);
    let base = config.economy.land.base_rate.money();
    for (i, block) in world.map().blocks.iter().enumerate() {
        let Some(lease) = block.lease.as_ref().filter(|l| mine(l.holder)) else {
            continue;
        };
        let Ok(i) = u32::try_from(i) else {
            continue;
        };
        let mult = config.map.tiers.get(block.tier).desirability_mult.rate();
        land.0 += world
            .businesses()
            .on_block(BlockId(i))
            .filter(|(_, b)| b.owner != lease.holder)
            .map(|(_, b)| land_rent_per_day(b.volume(), base, mult))
            .sum();
        land.1 += lease.last_price;
    }
    LineYields {
        retail: daily_yield(retail.0, retail.1),
        residential: daily_yield(residential.0, residential.1),
        land: daily_yield(land.0, land.1),
    }
}

/// True if `other` earns enough more than `home` to move (R66): `other` is positive and at
/// least `home × (1 + threshold)` (just `home` when `home` is negative). An unknown `other`
/// never wins; an unknown `home` counts as zero.
pub fn beats(other: Option<Rate>, home: Option<Rate>, threshold: Rate) -> bool {
    let Some(other) = other else {
        return false;
    };
    let home = home.map_or(0, Rate::bp);
    let margin =
        i128::from(home.max(0)) * i128::from(threshold.bp()) / i128::from(BASIS_POINTS_PER_UNIT);
    other.bp() > 0 && i128::from(other.bp()) >= i128::from(home) + margin
}

/// What it holds, by value: `(outside its home line, everything)`. Buildings count at build
/// cost (R41), in their line ([`line_of`]; a building in no line counts as home). For a land
/// dealer (`home` = [`Line::Land`]) its leases at book value (§3) and its cash in hand count
/// as home too: cash is the stock it trades with. For anyone else leases are the ground its
/// buildings stand on and cash is not a line, so neither counts.
pub fn holdings(world: &World, me: PlayerId, home: Line) -> (Money, Money) {
    let config = world.config();
    let (mut outside, mut all) = (Money::ZERO, Money::ZERO);
    for (_, b) in world.businesses().owned_by(AccountId::Player(me)) {
        all += b.build_cost;
        if line_of(config, b).is_some_and(|l| l != home) {
            outside += b.build_cost;
        }
    }
    if home == Line::Land {
        all += super::held_lots(world, me)
            .into_iter()
            .filter_map(|b| super::lease_book_value(world, b))
            .sum();
        all += world
            .ledger()
            .balance(AccountId::Player(me))
            .max(Money::ZERO);
    }
    (outside, all)
}

/// True if adding `extra` outside its home line keeps that share of its holdings within `cap`.
pub fn within_cap(world: &World, me: PlayerId, home: Line, extra: Money, cap: Rate) -> bool {
    let (outside, all) = holdings(world, me, home);
    let (outside, all) = (outside + extra, all + extra);
    all.pence() > 0
        && i128::from(outside.pence()) * i128::from(BASIS_POINTS_PER_UNIT)
            <= i128::from(cap.bp()) * i128::from(all.pence())
}

/// The day's reading of the yields: the market's and its own, taken once a day as soon as night
/// trade closes, when every shop's takings for the day are in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Outlook {
    taken_on: Option<u32>,
    market: LineYields,
    mine: LineYields,
}

impl Outlook {
    /// Takes today's reading if night trade has closed and it hasn't yet.
    pub fn observe(&mut self, world: &World, me: PlayerId) {
        let clock = world.clock();
        let today = clock.day();
        if clock.second_of_day() < clock.rules().trading.night.1 || self.taken_on == Some(today) {
            return;
        }
        self.taken_on = Some(today);
        self.market = line_yields(world, None);
        self.mine = line_yields(world, Some(AccountId::Player(me)));
    }

    /// True once it has a reading.
    pub fn has_reading(&self) -> bool {
        self.taken_on.is_some()
    }

    /// The whole city's yields at the last reading.
    pub fn market(&self) -> LineYields {
        self.market
    }

    /// Its own yields at the last reading.
    pub fn mine(&self) -> LineYields {
        self.mine
    }

    /// True if the market's `other` line beats its own `home` line (the market's, if it has
    /// nothing there) by `threshold` ([`beats`]). False before the first reading.
    pub fn favours(&self, home: Line, other: Line, threshold: Rate) -> bool {
        let own = self.mine.of(home).or(self.market.of(home));
        self.has_reading() && beats(self.market.of(other), own, threshold)
    }
}

/// An opportunist's memory: the day's [`Outlook`] and the rents on the flats it lets.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Opportunist {
    outlook: Outlook,
    rents: RentBook,
}

impl Opportunist {
    /// Takes the day's reading when it is due ([`Outlook::observe`]). Cheap on every other tick.
    pub fn observe(&mut self, world: &World, me: PlayerId) {
        self.outlook.observe(world, me);
    }

    /// The last reading.
    pub fn outlook(&self) -> &Outlook {
        &self.outlook
    }

    /// True if it is enabled and `other` is earning enough more than `home` (R66).
    pub fn wants(&self, cfg: &OpportunismConfig, home: Line, other: Line) -> bool {
        cfg.enabled && self.outlook.favours(home, other, cfg.threshold.rate())
    }

    /// Sets and reviews the rent on every apartment block it owns by vacancy (R51).
    pub fn let_flats(
        &mut self,
        world: &World,
        me: PlayerId,
        cfg: &OpportunismConfig,
    ) -> Vec<Command> {
        self.rents.review(world, me, &VacancyRule::from(cfg))
    }
}

/// An apartment block it may put up now on one of `lots` (in order): at most
/// `apartments_per_lot` a lease, at the first legal site, if it can pay, keeps `buffer` and stays
/// within its cap outside `home` ([`within_cap`]).
pub fn flats_plan(
    world: &World,
    me: PlayerId,
    cfg: &OpportunismConfig,
    home: Line,
    lots: &[BlockId],
    buffer: Money,
) -> Option<Plan> {
    let apartments = apartments_type(world.config(), world.clock().year())?;
    lots.iter()
        .filter(|block| owned_on(world, me, **block, |t| t == apartments) < cfg.apartments_per_lot)
        .filter_map(|block| {
            plan_on(
                world,
                me,
                *block,
                apartments,
                cfg.apartment_footprint,
                cfg.apartment_height,
                cfg.fit_out,
            )
        })
        .find(|plan| {
            spare(world, me, buffer) >= plan.cost
                && world.can_afford(me, plan.cost)
                && within_cap(world, me, home, plan.cost, cfg.max_outside_share.rate())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const QUARTER: Rate = Rate::from_bp(2_500);

    #[allow(
        clippy::unnecessary_wraps,
        reason = "yields are optional; tests pass them as such"
    )]
    fn bp(n: i64) -> Option<Rate> {
        Some(Rate::from_bp(n))
    }

    #[test]
    fn a_yield_is_net_over_capital() {
        let y = |net, cap| daily_yield(Money::from_pounds(net), Money::from_pounds(cap));
        assert_eq!(y(10, 100), bp(1_000));
        assert_eq!(y(-5, 100), bp(-500));
        assert_eq!(y(10, 0), None);
    }

    #[test]
    fn the_other_line_must_beat_home_by_the_threshold() {
        assert!(beats(bp(1_250), bp(1_000), QUARTER)); // exactly 25% more
        assert!(!beats(bp(1_249), bp(1_000), QUARTER));
        assert!(beats(bp(1), bp(-400), QUARTER)); // home losing: any profit wins
        assert!(beats(bp(1), None, QUARTER)); // nothing at home yet counts as zero
        assert!(!beats(bp(0), None, QUARTER)); // a line that earns nothing never wins
        assert!(!beats(bp(-10), bp(-400), QUARTER));
        assert!(!beats(None, bp(-400), QUARTER)); // nothing seen, nothing to move to
    }

    proptest! {
        /// A higher threshold never makes moving more likely.
        #[test]
        fn a_higher_threshold_never_moves_sooner(
            other in -5_000i64..5_000, home in -5_000i64..5_000, lo in 0i64..20_000, extra in 0i64..20_000,
        ) {
            let strict = beats(bp(other), bp(home), Rate::from_bp(lo + extra));
            prop_assert!(!strict || beats(bp(other), bp(home), Rate::from_bp(lo)));
        }
    }
}
