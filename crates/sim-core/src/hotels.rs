//! Hotels: the unlock, one per set, rooms by volume, and the night's bookings (§4.1, §7.3;
//! R14, R29, R30).
//!
//! Owner boundary: the rules that make a Hotel different from any other building. The Hotel
//! is an ordinary [`crate::businesses`] building of any type with `hours: hotel` (the Hotel,
//! and from 2005 the Boutique Hotel); this module says when it may be built and what it offers tourists.
//! Tourists choosing and checking in are [`crate::customers`]; money moves only through
//! [`crate::economy`] (the world pays each check-in from the shoppers' purse).
//!
//! - **Unlock (R14):** a Hotel may go up on a block only if the builder holds the lease on
//!   every block of that block's colour set ([`crate::land::Map::holds_full_set`]).
//! - **One per set (R29):** a set with a Hotel on any of its blocks, whoever owns it, gets no
//!   second one.
//! - **Rooms (R30):** `rooms = volume ÷ room_size`, like flats (R50). No stock: a room is
//!   capacity, sold in the hotel window (23:30–03:00) and free again the next night.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::businesses::trading::unit_price;
use crate::businesses::{Business, BusinessId, Businesses, LeverError};
use crate::clock::Clock;
use crate::config::Config;
use crate::customers::movement::{Cell, Geometry};
use crate::customers::scoring;
use crate::economy::AccountId;
use crate::land::{BlockId, Map};
use crate::money::{Money, Rate};
use crate::population::residential::fit_out_appeal;
use crate::population::{Class, Cohort};
use crate::rng::SimRng;

/// Why a Hotel may not be built (R14, R29).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HotelRefusal {
    /// The block belongs to no colour set.
    NotInASet,
    /// The builder doesn't hold the lease on every block of the set (R14).
    NeedsFullSet {
        /// The set.
        set: String,
    },
    /// The set already has its Hotel (R29: one per set).
    SetHasHotel {
        /// The set.
        set: String,
        /// The Hotel already there.
        hotel: BusinessId,
    },
}

/// Rooms in a Hotel: `volume ÷ room_size`, rounded down (R30).
pub fn rooms(volume: i64, room_size: u32) -> u32 {
    if room_size == 0 || volume <= 0 {
        return 0;
    }
    u32::try_from(volume / i64::from(room_size)).unwrap_or(u32::MAX)
}

/// Rooms in a Hotel building, by its volume and `hotel.room_size` (R30).
pub fn rooms_in(config: &Config, hotel: &Business) -> u32 {
    rooms(hotel.volume(), config.businesses.hotel.room_size)
}

/// True if `type_id` is a Hotel type: it trades in the hotel window (R30).
pub fn is_hotel(config: &Config, type_id: &str) -> bool {
    config
        .businesses
        .types
        .get(type_id)
        .is_some_and(|t| t.hours == crate::businesses::Hours::Hotel)
}

/// Hotels standing on any block of colour set `set`, in id order.
pub fn hotels_in_set(
    config: &Config,
    map: &Map,
    businesses: &Businesses,
    set: &str,
) -> Vec<BusinessId> {
    businesses
        .by_id
        .iter()
        .filter(|(_, b)| is_hotel(config, &b.type_id))
        .filter(|(_, b)| map.block(b.block).and_then(|k| k.set.as_deref()) == Some(set))
        .map(|(id, _)| *id)
        .collect()
}

/// May `builder` put a Hotel on `block`? (R14, R29.) Every other building rule (footprint,
/// height, the lease on this block) is checked as for any building.
pub fn check_build(
    config: &Config,
    map: &Map,
    businesses: &Businesses,
    block: BlockId,
    builder: AccountId,
) -> Result<(), HotelRefusal> {
    let set = map
        .block(block)
        .and_then(|b| b.set.clone())
        .ok_or(HotelRefusal::NotInASet)?;
    if let Some(&hotel) = hotels_in_set(config, map, businesses, &set).first() {
        return Err(HotelRefusal::SetHasHotel { set, hotel });
    }
    if !map.holds_full_set(builder, &set) {
        return Err(HotelRefusal::NeedsFullSet { set });
    }
    Ok(())
}

/// True if any Hotel stands in the city.
pub fn any_hotel(config: &Config, businesses: &Businesses) -> bool {
    businesses
        .by_id
        .values()
        .any(|b| is_hotel(config, &b.type_id))
}

/// Why a tourist at a Hotel's door didn't get a room.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NoRoom {
    /// Every room is taken tonight: a lost sale (§8.3).
    Full,
    /// Not a trading Hotel (shut, or not a Hotel).
    Closed,
    /// The room costs more than the tourist has left.
    TooDear,
}

/// Checks a tourist into `hotel` (R29, R30): it must be a Hotel trading now with a room free
/// tonight that the tourist can pay for. Takes the room (`guests`), records the takings and
/// returns the room price, which the world moves from the shoppers' purse to the owner:
/// revenue is booked at check-in. Never takes more rooms than [`rooms_in`].
///
/// # Errors
///
/// [`NoRoom`] says why not; nothing changes.
pub fn check_in(
    businesses: &mut Businesses,
    guests: &mut BTreeMap<BusinessId, u32>,
    config: &Config,
    clock: &Clock,
    hotel: BusinessId,
    budget: Money,
) -> Result<Money, NoRoom> {
    let b = businesses.by_id.get_mut(&hotel).ok_or(NoRoom::Closed)?;
    if !is_hotel(config, &b.type_id) || !b.is_trading(clock) {
        return Err(NoRoom::Closed);
    }
    let (price, _) = room_price(config, b).ok_or(NoRoom::Closed)?;
    let taken = guests.entry(hotel).or_insert(0);
    if *taken >= rooms_in(config, b) {
        return Err(NoRoom::Full);
    }
    if price > budget {
        return Err(NoRoom::TooDear);
    }
    *taken += 1;
    b.takings_today += price;
    let good = config.businesses.hotel.room_good.clone();
    *b.sold_today.entry(good).or_insert(0) += 1;
    Ok(price)
}

/// One Hotel's offer of a room tonight, as tourists score it (§7.3).
#[derive(Debug, Clone, PartialEq)]
pub struct HotelOffer {
    /// The Hotel.
    pub hotel: BusinessId,
    /// Its door.
    pub door: Cell,
    /// Price of a room night.
    pub price: Money,
    /// The reference room price.
    pub ref_price: Money,
    /// `class_affinity(class, tier) × fit_out_appeal` per class: the set's desirability and the
    /// fit-out (§7.3).
    pub affinity: [f64; 3],
    /// Rooms still free tonight.
    pub free: u32,
}

/// Every trading Hotel's offer tonight, in id order, given the rooms already taken.
pub fn hotel_offers(
    businesses: &Businesses,
    map: &Map,
    config: &Config,
    clock: &Clock,
    geometry: Geometry,
    guests: &BTreeMap<BusinessId, u32>,
) -> Vec<HotelOffer> {
    let mut out = Vec::new();
    for (id, b) in &businesses.by_id {
        if !is_hotel(config, &b.type_id) || !b.is_trading(clock) {
            continue;
        }
        let Some(block) = map.block(b.block) else {
            continue;
        };
        let Some((price, ref_price)) = room_price(config, b) else {
            continue;
        };
        let taken = guests.get(id).copied().unwrap_or(0);
        let appeal = fit_out_appeal(config, b.fit_out);
        out.push(HotelOffer {
            hotel: *id,
            door: geometry.door(block, b.footprint),
            price,
            ref_price,
            affinity: Class::ALL
                .map(|c| scoring::desirability(&config.customers, c, block.tier, appeal)),
            free: rooms_in(config, b).saturating_sub(taken),
        });
    }
    out
}

/// A Hotel's price for a room night and the reference room price: `hotel.room_good`'s
/// `ref_price`, times the Hotel's price lever for that good (100% until the owner sets one;
/// R29, R30).
pub fn room_price(config: &Config, hotel: &Business) -> Option<(Money, Money)> {
    let good = &config.businesses.hotel.room_good;
    let ref_price = config.businesses.goods.get(good)?.ref_price.money();
    let share = hotel.prices.get(good).copied().unwrap_or(Rate::ONE);
    Some((unit_price(share, ref_price), ref_price))
}

/// Sets a Hotel's room price as a share of the reference room price (§7.1's price lever,
/// R29). Hotels sell rooms, not goods, so this is the only price they take.
///
/// # Errors
///
/// Not a Hotel, or a price of zero or less.
pub fn set_room_price(
    businesses: &mut Businesses,
    config: &Config,
    hotel: BusinessId,
    price: Rate,
) -> Result<(), LeverError> {
    let b = businesses
        .by_id
        .get_mut(&hotel)
        .filter(|b| is_hotel(config, &b.type_id))
        .ok_or(LeverError::NoSuchBusiness(hotel))?;
    if price.bp() <= 0 {
        return Err(LeverError::BadPrice);
    }
    b.prices
        .insert(config.businesses.hotel.room_good.clone(), price);
    Ok(())
}

/// Picks a Hotel for a tourist (§7.3): every offer with a free room the tourist can afford is
/// scored `desirability × price_factor × distance_decay × reputation`, like a shop (§8.3),
/// then one is picked by weighted random. Returns the index into `offers`.
#[allow(clippy::too_many_arguments, reason = "the §8.3 inputs")]
pub fn choose_hotel(
    cohort: Cohort,
    at: Cell,
    budget: Money,
    offers: &[HotelOffer],
    reputation: &BTreeMap<BusinessId, Rate>,
    config: &Config,
    geometry: Geometry,
    rng: &mut SimRng,
) -> Option<usize> {
    let rules = &config.customers;
    let mut index = Vec::new();
    let mut scores = Vec::new();
    for (i, o) in offers.iter().enumerate() {
        if o.free == 0 || o.price > budget {
            continue;
        }
        index.push(i);
        scores.push(scoring::shop_score(
            config,
            cohort.class,
            o.affinity[cohort.class.index()],
            o.price,
            o.ref_price,
            false,
            geometry.blocks_between(at, o.door),
            reputation
                .get(&o.hotel)
                .copied()
                .unwrap_or_else(|| scoring::unit_to_rate(rules.reputation.initial)),
        ));
    }
    scoring::weighted_pick(&scores, rng).and_then(|k| index.get(k).copied())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::businesses::{BuildRequest, FitOutLevel};
    use crate::economy::PlayerId;
    use crate::economy::test_support::shipped_config;
    use crate::land::{Footprint, Lease};

    const P0: AccountId = AccountId::Player(PlayerId(0));
    const P1: AccountId = AccountId::Player(PlayerId(1));

    fn lease(map: &mut Map, key: &str, holder: AccountId) {
        let id = map.block_id(key).unwrap();
        map.blocks[id.0 as usize].lease = Some(Lease::new(holder, 1930, 30, Money::ZERO));
    }

    fn hotel_on(
        config: &Config,
        businesses: &mut Businesses,
        block: BlockId,
        height: u32,
    ) -> BusinessId {
        let kind = &config.businesses.types["hotel"];
        let request = BuildRequest {
            type_id: "hotel".to_owned(),
            block,
            footprint: Footprint {
                x: 0,
                y: 0,
                w: 3,
                d: 3,
            },
            height,
            owner: P0,
            fit_out: FitOutLevel::Medium,
        };
        let id = BusinessId(businesses.next_id);
        businesses.next_id += 1;
        businesses
            .by_id
            .insert(id, Business::new(request, kind, Money::ZERO));
        id
    }

    #[test]
    fn rooms_are_volume_over_room_size_rounded_down() {
        assert_eq!(rooms(72, 4), 18);
        assert_eq!(rooms(75, 4), 18);
        assert_eq!(rooms(3, 4), 0);
        assert_eq!(rooms(0, 4), 0);
        assert_eq!(rooms(-8, 4), 0);
        assert_eq!(rooms(72, 0), 0);
    }

    #[test]
    fn the_hotel_needs_every_block_of_the_set_and_only_one_per_set() {
        // The v1 board's light-blue set: Old Kent Road, Whitechapel, Bethnal Green.
        let config = crate::economy::test_support::shipped_config_v1();
        let mut map = Map::from_config(&config.map);
        let mut businesses = Businesses::default();
        let okr = map.block_id("old_kent_road").unwrap();
        let bethnal = map.block_id("bethnal_green").unwrap();
        lease(&mut map, "old_kent_road", P0);
        lease(&mut map, "whitechapel", P0);
        let set = "light_blue".to_owned();
        assert_eq!(
            check_build(&config, &map, &businesses, okr, P0),
            Err(HotelRefusal::NeedsFullSet { set: set.clone() })
        );
        lease(&mut map, "bethnal_green", P1);
        assert!(!map.holds_full_set(P0, &set));
        lease(&mut map, "bethnal_green", P0);
        assert!(map.holds_full_set(P0, &set));
        assert!(!map.holds_full_set(P0, "orange"));
        assert!(!map.holds_full_set(P0, "no_such_set"));
        assert_eq!(check_build(&config, &map, &businesses, okr, P0), Ok(()));
        let first = hotel_on(&config, &mut businesses, okr, 10);
        assert_eq!(
            check_build(&config, &map, &businesses, bethnal, P0),
            Err(HotelRefusal::SetHasHotel { set, hotel: first })
        );
        assert!(any_hotel(&config, &businesses));
    }

    #[test]
    fn check_in_never_takes_more_rooms_than_there_are() {
        let config = shipped_config();
        let map = Map::from_config(&config.map);
        let mut businesses = Businesses::default();
        let hotel = hotel_on(
            &config,
            &mut businesses,
            map.block_id("mayfair").unwrap(),
            4,
        );
        let mut clock = Clock::new(crate::clock::ClockRules::from_config(&config.game.clock));
        while clock.second_of_day() != 12 * 3600 {
            clock.advance();
        }
        let mut guests = BTreeMap::new();
        let cash = Money::from_pounds(100);
        assert_eq!(
            check_in(&mut businesses, &mut guests, &config, &clock, hotel, cash),
            Err(NoRoom::Closed),
            "shut at noon"
        );
        while clock.second_of_day() != 23 * 3600 + 45 * 60 {
            clock.advance();
        }
        let price = room_price(&config, &businesses.by_id[&hotel]).unwrap().0;
        assert_eq!(
            check_in(
                &mut businesses,
                &mut guests,
                &config,
                &clock,
                hotel,
                price - Money::from_pence(1)
            ),
            Err(NoRoom::TooDear)
        );
        for _ in 0..9 {
            assert_eq!(
                check_in(&mut businesses, &mut guests, &config, &clock, hotel, cash),
                Ok(price)
            );
        }
        assert_eq!(
            check_in(&mut businesses, &mut guests, &config, &clock, hotel, cash),
            Err(NoRoom::Full)
        );
        assert_eq!(guests[&hotel], 9);
        assert_eq!(
            businesses.by_id[&hotel].takings_today,
            price.saturating_mul(9)
        );
    }

    #[test]
    fn a_room_costs_the_reference_price_until_the_owner_sets_one() {
        let config = shipped_config();
        let map = Map::from_config(&config.map);
        let mut businesses = Businesses::default();
        let hotel = hotel_on(
            &config,
            &mut businesses,
            map.block_id("mayfair").unwrap(),
            4,
        );
        let reference = config.businesses.goods[&config.businesses.hotel.room_good]
            .ref_price
            .money();
        let price = |b: &Businesses| room_price(&config, &b.by_id[&hotel]).unwrap().0;
        assert_eq!(price(&businesses), reference);
        set_room_price(&mut businesses, &config, hotel, Rate::from_bp(15_000)).unwrap();
        assert_eq!(price(&businesses), Rate::from_bp(15_000).apply(reference));
        assert_eq!(
            set_room_price(&mut businesses, &config, hotel, Rate::ZERO),
            Err(LeverError::BadPrice)
        );
    }

    #[test]
    fn the_boutique_hotel_is_a_hotel_and_counts_towards_one_per_set() {
        let config = shipped_config();
        assert!(is_hotel(&config, "hotel"));
        assert!(is_hotel(&config, "boutique_hotel"));
        assert!(!is_hotel(&config, "bakery"));
        let mut map = Map::from_config(&config.map);
        for key in ["knightsbridge", "belgravia", "mayfair"] {
            lease(&mut map, key, P0);
        }
        let mut businesses = Businesses::default();
        let mayfair = map.block_id("mayfair").unwrap();
        let mut boutique = Business::new(
            BuildRequest {
                type_id: "boutique_hotel".into(),
                block: mayfair,
                footprint: Footprint {
                    x: 0,
                    y: 0,
                    w: 3,
                    d: 3,
                },
                height: 10,
                owner: P0,
                fit_out: FitOutLevel::High,
            },
            &config.businesses.types["boutique_hotel"],
            Money::ZERO,
        );
        boutique.owner = P1;
        businesses.by_id.insert(BusinessId(0), boutique);
        let belgravia = map.block_id("belgravia").unwrap();
        assert!(matches!(
            check_build(&config, &map, &businesses, belgravia, P0),
            Err(HotelRefusal::SetHasHotel { .. })
        ));
    }
}
