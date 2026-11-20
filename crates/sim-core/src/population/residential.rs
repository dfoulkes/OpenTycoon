//! Residential: flats, owner-set rents, the 20%/50% affordability rule, the 05:30
//! house-hunt and the once-a-decade move (§7.2; R50–R54, R56).
//!
//! Apartment blocks are businesses of the type `residential.building_type` (default
//! `apartments`). This module never touches their money: it reports rent owed
//! ([`rent_due`]), which economy settles at 06:00, and the volume-based running costs every
//! building pays ([`apartment_daily_costs`], R56).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{Cohort, Population, allowance};
use crate::businesses::{BusinessId, Businesses, FitOutLevel, stock_capacity};
use crate::config::Config;
use crate::config::CustomersConfig;
use crate::config::customers::ResidentialSettings;
use crate::customers::scoring;
use crate::economy::{AccountId, PlayerId};
use crate::land::{BlockId, Map, Tier, land_rent_per_day, volume};
use crate::money::{Money, Rate};
use crate::rng::{SimRng, below};
use crate::utilities::{UtilityKind, utility_bill};

/// A stable resident id.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
pub struct ResidentId(pub u64);

/// Someone who lives in a flat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Resident {
    /// Band, sex and class.
    pub cohort: Cohort,
    /// The apartment block they live in.
    pub home: BusinessId,
    /// The year they moved in (R54: the next move is at least `move_interval_years` later).
    pub moved_year: i32,
}

/// Residents and the rents owners have set.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Residential {
    /// Residents in id order.
    pub residents: BTreeMap<ResidentId, Resident>,
    /// Rent per flat per day, by building, once the owner has set it (R51).
    pub rents: BTreeMap<BusinessId, Money>,
    /// Next id to hand out.
    pub next_id: u64,
}

impl Residential {
    /// A building's rent per flat per day: the owner's, or the default (R51).
    pub fn rent(&self, building: BusinessId, default: Money) -> Money {
        self.rents.get(&building).copied().unwrap_or(default)
    }

    /// Flats let per building.
    pub fn occupancy(&self) -> BTreeMap<BusinessId, u32> {
        let mut out = BTreeMap::new();
        for r in self.residents.values() {
            *out.entry(r.home).or_insert(0) += 1;
        }
        out
    }

    /// Moves someone in and returns their id.
    pub fn house(&mut self, cohort: Cohort, home: BusinessId, year: i32) -> ResidentId {
        let id = ResidentId(self.next_id);
        self.next_id += 1;
        self.residents.insert(
            id,
            Resident {
                cohort,
                home,
                moved_year: year,
            },
        );
        id
    }
}

/// Why a rent change was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RentError {
    /// No such building, or it is not an apartment block.
    NotApartments,
    /// Only the building's owner sets its rent.
    NotOwner,
    /// Rent can't be negative.
    Negative,
}

/// The owner sets a building's rent per flat per day (R51).
pub fn set_rent(
    residential: &mut Residential,
    businesses: &Businesses,
    apartment_type: &str,
    player: PlayerId,
    building: BusinessId,
    rent: Money,
) -> Result<(), RentError> {
    let b = businesses
        .by_id
        .get(&building)
        .filter(|b| b.type_id == apartment_type)
        .ok_or(RentError::NotApartments)?;
    if b.owner != AccountId::Player(player) {
        return Err(RentError::NotOwner);
    }
    if rent.is_negative() {
        return Err(RentError::Negative);
    }
    residential.rents.insert(building, rent);
    Ok(())
}

/// `flats = building_volume ÷ flat_size`, rounded down (§7.2, R50).
pub fn flats(volume: i64, flat_size: u32) -> u32 {
    if flat_size == 0 || volume <= 0 {
        return 0;
    }
    u32::try_from(volume / i64::from(flat_size)).unwrap_or(u32::MAX)
}

/// An apartment block as house-hunters see it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FlatListing {
    /// The building.
    pub building: BusinessId,
    /// Its block.
    pub block: BlockId,
    /// Who collects the rent.
    pub owner: AccountId,
    /// The block's desirability tier.
    pub tier: Tier,
    /// The building's fit-out appeal (`businesses.yaml` `fit_out.<level>.appeal`).
    pub appeal: Rate,
    /// Flats in the building (R50).
    pub flats: u32,
    /// Rent per flat per day (R51).
    pub rent: Money,
}

/// Every apartment block on the map, in building id order.
pub fn listings(
    businesses: &Businesses,
    map: &Map,
    config: &Config,
    residential: &Residential,
) -> Vec<FlatListing> {
    let rules = &config.customers.residential;
    businesses
        .by_id
        .iter()
        .filter(|(_, b)| b.type_id == rules.building_type)
        .filter_map(|(id, b)| {
            let tier = map.block(b.block)?.tier;
            Some(FlatListing {
                building: *id,
                block: b.block,
                owner: b.owner,
                tier,
                appeal: fit_out_appeal(config, b.fit_out),
                flats: flats(volume(b.height, b.footprint), rules.flat_size),
                rent: residential.rent(*id, rules.default_rent.money()),
            })
        })
        .collect()
}

/// A fit-out level's appeal multiplier (§7.1).
pub fn fit_out_appeal(config: &Config, level: FitOutLevel) -> Rate {
    let f = &config.businesses.fit_out;
    match level {
        FitOutLevel::Low => f.low.appeal,
        FitOutLevel::Medium => f.medium.appeal,
        FitOutLevel::High => f.high.appeal,
    }
    .rate()
}

/// How a flat's rent sits against someone's allowance (R52).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Affordability {
    /// Rent ≤ `comfortable_share` (20%) of the allowance.
    Comfortable,
    /// Rent ≤ `stretch_share` (50%), and the flat is desirable enough to justify it.
    Stretch,
    /// Neither.
    Unaffordable,
}

/// The R52 rule for one flat: comfortable at ≤ 20% of the allowance; a stretch up to 50% only
/// when `desirability ≥ stretch_min_desirability`; otherwise unaffordable.
pub fn affordability(
    rent: Money,
    allowance: Money,
    desirability: f64,
    rules: &ResidentialSettings,
) -> Affordability {
    if rent <= rules.comfortable_share.rate().apply(allowance) {
        Affordability::Comfortable
    } else if rent <= rules.stretch_share.rate().apply(allowance)
        && desirability >= rules.stretch_min_desirability
    {
        Affordability::Stretch
    } else {
        Affordability::Unaffordable
    }
}

/// The outcome of one person's house-hunt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HouseHunt {
    /// They take a flat in the listing at this index.
    Housed(usize),
    /// The cheapest vacant flat is over 20% of their allowance: they commute or visit (R52).
    PricedOut,
    /// No flat is vacant (or none appeals at all).
    NoVacancy,
}

fn vacant(listing: &FlatListing, occupancy: &BTreeMap<BusinessId, u32>) -> bool {
    listing.flats > occupancy.get(&listing.building).copied().unwrap_or(0)
}

/// Indices of the vacant listings this person would consider (R52): none at all if the
/// cheapest vacant flat is over `comfortable_share`; otherwise every vacant flat that is
/// comfortable or a justified stretch.
pub fn candidates(
    cohort: Cohort,
    listings: &[FlatListing],
    occupancy: &BTreeMap<BusinessId, u32>,
    rules: &CustomersConfig,
) -> Result<Vec<usize>, HouseHunt> {
    candidates_except(cohort, listings, occupancy, rules, None)
}

fn candidates_except(
    cohort: Cohort,
    listings: &[FlatListing],
    occupancy: &BTreeMap<BusinessId, u32>,
    rules: &CustomersConfig,
    except: Option<BusinessId>,
) -> Result<Vec<usize>, HouseHunt> {
    let budget = allowance(rules, cohort);
    let open: Vec<usize> = listings
        .iter()
        .enumerate()
        .filter(|(_, l)| Some(l.building) != except && vacant(l, occupancy))
        .map(|(i, _)| i)
        .collect();
    let cheapest = open
        .iter()
        .map(|i| listings[*i].rent)
        .min()
        .ok_or(HouseHunt::NoVacancy)?;
    let r = &rules.residential;
    if cheapest > r.comfortable_share.rate().apply(budget) {
        return Err(HouseHunt::PricedOut);
    }
    Ok(open
        .into_iter()
        .filter(|i| {
            let l = &listings[*i];
            let d = scoring::desirability(rules, cohort.class, l.tier, l.appeal);
            affordability(l.rent, budget, d, r) != Affordability::Unaffordable
        })
        .collect())
}

fn score(rules: &CustomersConfig, cohort: Cohort, l: &FlatListing) -> f64 {
    scoring::flat_score(rules, cohort.class, l.tier, l.appeal, l.rent)
}

/// One person's house-hunt: among [`candidates`], a weighted random pick by the business
/// score `desirability × price_factor(rent)` (R51, §8.3), not the top score.
pub fn choose_flat(
    cohort: Cohort,
    listings: &[FlatListing],
    occupancy: &BTreeMap<BusinessId, u32>,
    rules: &CustomersConfig,
    rng: &mut SimRng,
) -> HouseHunt {
    match candidates(cohort, listings, occupancy, rules) {
        Err(why) => why,
        Ok(idx) => {
            let scores: Vec<f64> = idx
                .iter()
                .map(|i| score(rules, cohort, &listings[*i]))
                .collect();
            scoring::weighted_pick(&scores, rng)
                .and_then(|k| idx.get(k).copied())
                .map_or(HouseHunt::NoVacancy, HouseHunt::Housed)
        }
    }
}

/// A resident's move (R54): among flats elsewhere that pass the same affordability rule,
/// those scoring strictly better than home, picked by weighted random. `None` to stay.
pub fn better_flat(
    resident: &Resident,
    listings: &[FlatListing],
    occupancy: &BTreeMap<BusinessId, u32>,
    rules: &CustomersConfig,
    rng: &mut SimRng,
) -> Option<usize> {
    let home = listings
        .iter()
        .find(|l| l.building == resident.home)
        .map_or(0.0, |l| score(rules, resident.cohort, l));
    let idx = candidates_except(
        resident.cohort,
        listings,
        occupancy,
        rules,
        Some(resident.home),
    )
    .ok()?;
    let (better, scores): (Vec<usize>, Vec<f64>) = idx
        .into_iter()
        .map(|i| (i, score(rules, resident.cohort, &listings[i])))
        .filter(|(_, s)| *s > home)
        .unzip();
    scoring::weighted_pick(&scores, rng).and_then(|k| better.get(k).copied())
}

/// What the 05:30 house-hunt did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct ArrivalsReport {
    /// People who came off the train.
    pub arrived: u64,
    /// Arrivals who found a flat.
    pub housed: u64,
    /// Arrivals priced out or finding nothing vacant: commuters or tourists today (R52).
    pub unhoused: u64,
    /// People already in the city, unhoused, who found a flat on a re-hunt.
    pub rehoused: u64,
    /// Residents who moved flat (R54).
    pub moved: u64,
}

/// Every person in a set of cohort counts, in a seeded random order (Fisher–Yates).
fn shuffled_people(counts: &BTreeMap<Cohort, u64>, rng: &mut SimRng) -> Vec<Cohort> {
    let mut people: Vec<Cohort> = counts
        .iter()
        .flat_map(|(c, n)| std::iter::repeat_n(*c, usize::try_from(*n).unwrap_or(0)))
        .collect();
    for i in (1..people.len()).rev() {
        let j = usize::try_from(below(rng, i as u64 + 1)).unwrap_or(0);
        people.swap(i, j);
    }
    people
}

/// Houses as many of `people` as will take a flat. A cohort that fails once fails for the
/// rest of the pass (flats only fill up), so it is skipped without re-scoring. Returns the
/// people left unhoused, in order.
fn house_all(
    people: Vec<Cohort>,
    residential: &mut Residential,
    listings: &[FlatListing],
    occupancy: &mut BTreeMap<BusinessId, u32>,
    rules: &CustomersConfig,
    year: i32,
    rng: &mut SimRng,
) -> Vec<Cohort> {
    let mut failed: BTreeSet<Cohort> = BTreeSet::new();
    let mut left = Vec::new();
    for cohort in people {
        if failed.contains(&cohort) {
            left.push(cohort);
            continue;
        }
        match choose_flat(cohort, listings, occupancy, rules, rng) {
            HouseHunt::Housed(i) => {
                let building = listings[i].building;
                residential.house(cohort, building, year);
                *occupancy.entry(building).or_insert(0) += 1;
            }
            HouseHunt::PricedOut | HouseHunt::NoVacancy => {
                failed.insert(cohort);
                left.push(cohort);
            }
        }
    }
    left
}

/// The 05:30 phase (R53, R54): the train's arrivals house-hunt first, in a seeded random
/// order; then (if `rehunt_unhoused`) the city's unhoused look again while flats stand empty;
/// then residents whose move window is open move if somewhere scores better and passes the
/// same affordability rule. Everyone not housed joins the pool as a commuter or tourist.
pub fn arrivals(
    pop: &mut Population,
    listings: &[FlatListing],
    rules: &CustomersConfig,
    year: i32,
    rng: &mut SimRng,
) -> ArrivalsReport {
    let mut report = ArrivalsReport::default();
    let mut occupancy = pop.residential.occupancy();

    // 1. The train's arrivals look for a flat first.
    let pending = std::mem::take(&mut pop.pending);
    let people = shuffled_people(&pending, rng);
    report.arrived = people.len() as u64;
    let left = house_all(
        people,
        &mut pop.residential,
        listings,
        &mut occupancy,
        rules,
        year,
        rng,
    );
    report.unhoused = left.len() as u64;
    report.housed = report.arrived - report.unhoused;
    for cohort in left {
        *pop.pool.entry(cohort).or_insert(0) += 1;
    }

    // 2. The city's unhoused try again while anything stands empty.
    if rules.residential.rehunt_unhoused && listings.iter().any(|l| vacant(l, &occupancy)) {
        let people = shuffled_people(&pop.pool, rng);
        let before = people.len() as u64;
        let left = house_all(
            people,
            &mut pop.residential,
            listings,
            &mut occupancy,
            rules,
            year,
            rng,
        );
        report.rehoused = before - left.len() as u64;
        pop.pool.clear();
        for cohort in left {
            *pop.pool.entry(cohort).or_insert(0) += 1;
        }
    }

    // 3. Residents whose move window is open (R54).
    let interval = i64::from(rules.residential.move_interval_years);
    let movers: Vec<ResidentId> = pop
        .residential
        .residents
        .iter()
        .filter(|(_, r)| i64::from(year) - i64::from(r.moved_year) >= interval)
        .map(|(id, _)| *id)
        .collect();
    for id in movers {
        let Some(r) = pop.residential.residents.get(&id).copied() else {
            continue;
        };
        if let Some(i) = better_flat(&r, listings, &occupancy, rules, rng) {
            let to = listings[i].building;
            if let Some(n) = occupancy.get_mut(&r.home) {
                *n = n.saturating_sub(1);
            }
            *occupancy.entry(to).or_insert(0) += 1;
            if let Some(m) = pop.residential.residents.get_mut(&id) {
                m.home = to;
                m.moved_year = year;
            }
            report.moved += 1;
        }
    }
    report
}

/// Rent owed to one landlord for one building at the 06:00 settlement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RentDue {
    /// The building.
    pub building: BusinessId,
    /// Who is owed.
    pub landlord: AccountId,
    /// Flats let.
    pub flats_let: u32,
    /// `flats_let × rent`.
    pub amount: Money,
}

/// What residents owe at 06:00, per building in id order (R51; economy debits it, §7.2).
/// The payers are the residents themselves: their rent comes out of their allowance first.
pub fn rent_due(residential: &Residential, listings: &[FlatListing]) -> Vec<RentDue> {
    let occupancy = residential.occupancy();
    listings
        .iter()
        .filter_map(|l| {
            let flats_let = occupancy.get(&l.building).copied().unwrap_or(0);
            (flats_let > 0).then(|| RentDue {
                building: l.building,
                landlord: l.owner,
                flats_let,
                amount: l.rent.saturating_mul(i64::from(flats_let)),
            })
        })
        .collect()
}

/// An apartment block's daily running costs (R56).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct ApartmentCosts {
    /// Land rent by the volume formula (§5.3).
    pub land_rent: Money,
    /// All utility bills by the volume formula (§5.3).
    pub utilities: Money,
    /// Stock it can hold: always zero for apartments.
    pub stock_capacity: i64,
}

/// An apartment block's land rent and utilities by the same volume formulas as every building,
/// and its stock capacity (none) (§7.2, R56). `None` if the building is not on the map or its
/// type is unknown. Telecoms is billed at the telephone rate (the 1995 internet rate is
/// post-v1, R11).
pub fn apartment_daily_costs(
    businesses: &Businesses,
    building: BusinessId,
    map: &Map,
    config: &Config,
) -> Option<ApartmentCosts> {
    let b = businesses.by_id.get(&building)?;
    let kind = config.businesses.types.get(&b.type_id)?;
    let tier = map.block(b.block)?.tier;
    let v = volume(b.height, b.footprint);
    let land_rent = land_rent_per_day(
        v,
        config.map.land_base_rate.money(),
        config.map.tiers.get(tier).desirability_mult.rate(),
    );
    let rates = &config.utilities.base_rates;
    let f = &kind.utility_factor;
    let utilities = UtilityKind::ALL
        .iter()
        .map(|u| {
            let (rate, factor) = match u {
                UtilityKind::Electricity => (rates.electricity, f.electricity),
                UtilityKind::Gas => (rates.gas, f.gas),
                UtilityKind::Water => (rates.water, f.water),
                UtilityKind::Telecoms => (rates.telecoms, f.telecoms),
            };
            utility_bill(v, rate.money(), factor.rate())
        })
        .sum();
    Some(ApartmentCosts {
        land_rent,
        utilities,
        stock_capacity: stock_capacity(b.height, b.footprint, kind.stock_density.rate()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flats_round_down_and_never_divide_by_zero() {
        assert_eq!(flats(108, 4), 27);
        assert_eq!(flats(3, 4), 0);
        assert_eq!(flats(108, 0), 0);
        assert_eq!(flats(-8, 4), 0);
    }

    #[test]
    fn rent_defaults_until_the_owner_sets_it() {
        let mut r = Residential::default();
        assert_eq!(
            r.rent(BusinessId(1), Money::from_pounds(3)),
            Money::from_pounds(3)
        );
        r.rents.insert(BusinessId(1), Money::from_pounds(5));
        assert_eq!(
            r.rent(BusinessId(1), Money::from_pounds(3)),
            Money::from_pounds(5)
        );
    }
}
