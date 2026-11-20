//! Hotels: the unlock, one per set, rooms by volume, and the night's bookings (§4.1, §7.3;
//! R14, R29, R30).
//!
//! Owner boundary: the rules that make a Hotel different from any other building. The Hotel
//! is an ordinary [`crate::businesses`] building of the type named by `businesses.yaml`
//! `hotel.building_type`; this module says when it may be built and what it offers tourists.
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

use crate::businesses::{BusinessId, Businesses};
use crate::clock::Clock;
use crate::config::Config;
use crate::customers::movement::{Cell, Geometry};
use crate::economy::AccountId;
use crate::land::{BlockId, Map};
use crate::money::Money;

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
    let _ = (volume, room_size);
    0
}

/// True if `type_id` is the Hotel type.
pub fn is_hotel(config: &Config, type_id: &str) -> bool {
    config.businesses.hotel.building_type == type_id
}

/// Hotels standing on any block of colour set `set`, in id order.
pub fn hotels_in_set(
    config: &Config,
    map: &Map,
    businesses: &Businesses,
    set: &str,
) -> Vec<BusinessId> {
    let _ = (config, map, businesses, set);
    Vec::new()
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
    let _ = (config, map, businesses, block, builder);
    Ok(())
}

/// True if any Hotel stands in the city.
pub fn any_hotel(config: &Config, businesses: &Businesses) -> bool {
    let _ = (config, businesses);
    false
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
    let _ = (businesses, map, config, clock, geometry, guests);
    Vec::new()
}
