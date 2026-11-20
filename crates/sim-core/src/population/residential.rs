//! Residential: flats, owner-set rents, the 20%/50% affordability rule, the 05:30
//! house-hunt and the once-a-decade move (§7.2; R50–R54, R56).
//!
//! Apartment blocks are businesses of the type `residential.building_type` (default
//! `apartments`). This module never touches their money: it reports rent owed
//! ([`rent_due`]), which economy settles at 06:00, and the volume-based running costs every
//! building pays ([`apartment_daily_costs`], R56).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{Cohort, Population, allowance};
use crate::businesses::{BusinessId, Businesses, FitOutLevel, stock_capacity};
use crate::config::Config;
use crate::config::CustomersConfig;
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
    let _ = (
        residential,
        businesses,
        apartment_type,
        player,
        building,
        rent,
    );
    Ok(())
}

/// `flats = building_volume ÷ flat_size`, rounded down (§7.2, R50).
pub fn flats(volume: i64, flat_size: u32) -> u32 {
    let _ = (volume, flat_size);
    0
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
    rules: &crate::config::customers::ResidentialSettings,
) -> Affordability {
    let _ = (rent, allowance, desirability, rules);
    Affordability::Unaffordable
}

/// The outcome of one person's house-hunt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HouseHunt {
    /// They take a flat in the listing at this index.
    Housed(usize),
    /// The cheapest vacant flat is over 20% of their allowance: they commute or visit (R52).
    PricedOut,
    /// No flat is vacant.
    NoVacancy,
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
    let _ = (cohort, listings, occupancy, rules);
    Err(HouseHunt::NoVacancy)
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
    let _ = (cohort, listings, occupancy, rules, rng);
    HouseHunt::NoVacancy
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
    let _ = (pop, listings, rules, year, rng);
    ArrivalsReport::default()
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
    let _ = (residential, listings);
    Vec::new()
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
/// type is unknown.
pub fn apartment_daily_costs(
    businesses: &Businesses,
    building: BusinessId,
    map: &Map,
    config: &Config,
) -> Option<ApartmentCosts> {
    let _ = (businesses, building, map, config);
    None
}

#[allow(dead_code, reason = "used once the stubs are filled in")]
fn unused_imports_guard(
    rng: &mut SimRng,
) -> (
    u64,
    fn(i64, Money, Rate) -> Money,
    fn(i64, Money, Rate) -> Money,
) {
    let _ = (
        allowance,
        scoring::price_factor,
        UtilityKind::ALL,
        stock_capacity,
    );
    (below(rng, 1), land_rent_per_day, utility_bill)
}
