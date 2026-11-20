//! `ai.yaml`: AI personalities and their weights (§10).

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::types::{ClockTime, Pounds, Ratio};
use crate::businesses::FitOutLevel;

/// Root of `ai.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AiConfig {
    /// Ticks between an AI player's decisions.
    #[schemars(range(min = 1, max = 100_000))]
    pub decision_interval_ticks: u32,
    /// Personality ids seated by default (v1: one Grocer).
    pub opponents: Vec<String>,
    /// Personalities, keyed by id. All share one evaluation function with different weights.
    pub personalities: BTreeMap<String, Personality>,
    /// How the Grocer plays (§10: "opens many cheap Low-tier shops"). Its tier choice comes
    /// from the `grocer` personality's weights; these are its operating rules.
    pub grocer: GrocerConfig,
    /// How the Landlord plays (§10: "hoards leases and never builds retail").
    pub landlord: LandlordConfig,
    /// How the Toff plays (§10: "only goes for High tier").
    pub toff: ToffConfig,
    /// How the Speculator plays (§10: "bids hard and flips").
    pub speculator: SpeculatorConfig,
    /// How Sir Penny Sugar plays (§10: "deals mostly in residential", §7.2).
    pub sir_penny_sugar: PennySugarConfig,
}

/// The Grocer's tunables (§10). Money is decimal pounds; shares are decimals.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GrocerConfig {
    /// Tiers whose `grocer` personality weight is at least this are worth bidding on
    /// (`tier_low: 1.0` passes, `tier_high: 0.1` does not). Between 0 and 1.
    pub min_tier_weight: Ratio,
    /// The most it will pay for a lot, as a share of its bid ceiling (`balance + credit
    /// limit`, R7). Above 0, at most 1.
    pub max_bid_share: Ratio,
    /// Cash it keeps in hand: it never spends (land, buildings) below this balance, so the
    /// 06:00 bills don't take it under zero by midnight (R3).
    pub cash_buffer: Pounds,
    /// Day-trade shop types it builds, in rotation (`businesses.yaml` ids).
    pub shop_types: Vec<String>,
    /// Footprint of each shop, `[width, depth]` in lot units.
    pub shop_footprint: [u32; 2],
    /// Height of each shop in units; capped at the type's `max_height` (R58).
    #[schemars(range(min = 1, max = 100))]
    pub shop_height: u32,
    /// Working-class bias: price this far under the reference price (0.05 = 95% of
    /// `ref_price`). At least 0, below 1.
    pub working_class_discount: Ratio,
    /// Random spread either side of its price, from its own RNG stream. Below `1 −
    /// working_class_discount`.
    pub price_jitter: Ratio,
    /// A new shop's first order, as a share of its capacity. Above 0, at most 1.
    pub initial_order_share: Ratio,
    /// Tomorrow's order = today's sales × (1 + this), capped at capacity (R2, R10). At most 1.
    pub order_margin: Ratio,
    /// From this time until midnight, a negative balance makes it sell a shop back to the
    /// city (R5) rather than take a strike (R3). Must be after day trade closes.
    pub rescue_after: ClockTime,
    /// When it moves into another line of business (R66). Shipped off: the Grocer stays pure
    /// retail.
    pub opportunism: OpportunismConfig,
}

/// The Landlord's tunables (§10). Tiers come from the `landlord` personality's weights.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LandlordConfig {
    /// Strength knob: the most it pays for one lease, as a share of its bid ceiling
    /// (`balance + credit limit`, R7). Above 0, at most 1.
    pub aggression: Ratio,
    /// Cash it never spends below (R3).
    pub cash_buffer: Pounds,
    /// Spread of its keenness: each auction it scales its maximum bid by a factor drawn from
    /// its own RNG stream in `[1 − bid_jitter, 1]`. Between 0 and 1.
    pub bid_jitter: Ratio,
    /// Tiers whose personality weight is at least this are worth a lease. Between 0 and 1.
    pub min_tier_weight: Ratio,
    /// Leases it holds at most at once.
    #[schemars(range(min = 1, max = 1000))]
    pub max_leases: u32,
    /// Footprint of each apartment block it builds, `[width, depth]` in lot units.
    pub apartment_footprint: [u32; 2],
    /// Height of each apartment block; capped at the type's `max_height` (R58).
    #[schemars(range(min = 1, max = 100))]
    pub apartment_height: u32,
    /// Apartment blocks it puts up per lease at most (0: pure land, no building at all).
    #[schemars(range(max = 81))]
    pub apartments_per_lot: u32,
    /// A new block's rent, as a share of `residential.reference_rent` (R51).
    pub start_rent: Ratio,
    /// How far it moves the rent each morning, as a share of `reference_rent`. Above 0.
    pub rent_step: Ratio,
    /// Occupancy (flats let ÷ flats) at or above which it raises the rent. At most 1.
    pub raise_at: Ratio,
    /// Occupancy at or below which it cuts the rent. Below `raise_at`.
    pub cut_at: Ratio,
    /// The lowest rent it sets, as a share of `reference_rent`. At most `max_rent`.
    pub min_rent: Ratio,
    /// The highest rent it sets, as a share of `reference_rent`.
    pub max_rent: Ratio,
    /// Buys the city's apartment blocks at auction (R28), up to their build cost, in
    /// competition with Sir Penny Sugar (R67).
    pub buy_city_blocks: bool,
    /// Late in a lease (this many years or fewer left) it offers the lease at auction (R33)
    /// when the reserve beats what holding to expiry is worth. 0 never sells.
    #[schemars(range(max = 1000))]
    pub sell_within_years: u32,
    /// From this time to midnight, a negative balance makes it sell (R5) or offer a lease.
    pub rescue_after: ClockTime,
}

/// The Toff's tunables (§10). Tiers come from the `toff` personality's weights.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToffConfig {
    /// Strength knob: the most it pays for one lease, as a share of its bid ceiling (R7).
    /// Above 0, at most 1.
    pub aggression: Ratio,
    /// Cash it never spends below (R3): large, by temperament.
    pub cash_buffer: Pounds,
    /// Spread of its keenness: each auction it scales its maximum bid by a factor drawn from
    /// its own RNG stream in `[1 − bid_jitter, 1]`. Between 0 and 1.
    pub bid_jitter: Ratio,
    /// Tiers whose personality weight is at least this are worth a lease (High only).
    pub min_tier_weight: Ratio,
    /// Shop types it prefers, best first (`businesses.yaml` ids). Only types unlocked and not
    /// obsolete in the current year are built; when none of these is, it builds any available
    /// retail type, dearest goods first.
    pub shop_types: Vec<String>,
    /// Footprint of each shop, `[width, depth]` in lot units.
    pub shop_footprint: [u32; 2],
    /// Height of each shop; capped at the type's `max_height` (R58).
    #[schemars(range(min = 1, max = 100))]
    pub shop_height: u32,
    /// Shops it builds per lease at most: fewer, finer shops.
    #[schemars(range(min = 1, max = 81))]
    pub shops_per_lot: u32,
    /// Fit-out of every shop (§7.1).
    pub fit_out: FitOutLevel,
    /// Prices this far over the reference price (0.3 = 130% of `ref_price`). Between 0 and 4.
    pub price_premium: Ratio,
    /// A new shop's first order, as a share of capacity. Above 0, at most 1.
    pub initial_order_share: Ratio,
    /// Tomorrow's order = today's sales × (1 + this), capped at capacity (R2, R10). At most 1.
    pub order_margin: Ratio,
    /// From this time to midnight, a negative balance makes it sell a shop to the city (R5).
    pub rescue_after: ClockTime,
    /// When it moves into luxury apartments on its High-tier lots (R66).
    pub opportunism: OpportunismConfig,
}

/// The Speculator's tunables (§10). Tiers come from the `speculator` personality's weights.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SpeculatorConfig {
    /// Strength (and risk) knob: the most it pays for one lease, as a share of its bid
    /// ceiling (R7), which includes its credit. Above 0, at most 1.
    pub aggression: Ratio,
    /// Cash it keeps when it opens an auction: small, by temperament.
    pub cash_buffer: Pounds,
    /// Spread of its keenness: each auction it scales its maximum bid by a factor drawn from
    /// its own RNG stream in `[1 − bid_jitter, 1]`. Between 0 and 1.
    pub bid_jitter: Ratio,
    /// Tiers whose personality weight is at least this are worth a punt. Between 0 and 1.
    pub min_tier_weight: Ratio,
    /// On a fresh lease from the city, the most it pays over the land base price (0.25 = 125%
    /// of base): it has to sell on for more. Between 0 and 10.
    pub max_markup: Ratio,
    /// Leases and businesses it holds at most at once.
    #[schemars(range(min = 1, max = 1000))]
    pub max_holdings: u32,
    /// Game hours it holds a purchase before putting it back up for auction.
    #[schemars(range(max = 10_000))]
    pub hold_hours: u32,
    /// On an item a player offers (a lease part-way through its term, or a business), the most
    /// it bids as a share of book value (land value, R39; build cost, R41). Above 0, at most 1.
    pub bargain_share: Ratio,
    /// From this time to midnight, a negative balance makes it sell (R5) or offer a lease.
    pub rescue_after: ClockTime,
    /// Game hours it holds an apartment block before re-listing it: long enough to collect
    /// some rent (R51) first. Otherwise `hold_hours` applies.
    #[schemars(range(max = 10_000))]
    pub apartment_hold_hours: u32,
    /// When it buys, or builds and flips, apartment blocks (R66).
    pub opportunism: OpportunismConfig,
}

/// Sir Penny Sugar's tunables (§10, §7.2). Tiers come from the `sir_penny_sugar` weights.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PennySugarConfig {
    /// Strength knob: the most he pays for one lease, as a share of his bid ceiling (R7).
    /// Above 0, at most 1.
    pub aggression: Ratio,
    /// Cash he never spends below (R3).
    pub cash_buffer: Pounds,
    /// Spread of its keenness: each auction it scales its maximum bid by a factor drawn from
    /// its own RNG stream in `[1 − bid_jitter, 1]`. Between 0 and 1.
    pub bid_jitter: Ratio,
    /// Tiers whose personality weight is at least this are worth a lease. Between 0 and 1.
    pub min_tier_weight: Ratio,
    /// Extra share of his bid ceiling he pays for a lot with an open tube station (§4.3). He
    /// also bids on station lots first. Between 0 and 1.
    pub station_premium: Ratio,
    /// Footprint of each apartment block, `[width, depth]` in lot units.
    pub apartment_footprint: [u32; 2],
    /// Height of each apartment block; capped at the type's `max_height` (R58).
    #[schemars(range(min = 1, max = 100))]
    pub apartment_height: u32,
    /// Apartment blocks he puts up per lease at most.
    #[schemars(range(min = 1, max = 81))]
    pub apartments_per_lot: u32,
    /// A new block's rent, as a share of `residential.reference_rent` (R51).
    pub start_rent: Ratio,
    /// How far he moves the rent each morning, as a share of `reference_rent`. Above 0.
    pub rent_step: Ratio,
    /// Occupancy (flats let ÷ flats) at or above which he raises the rent. At most 1.
    pub raise_at: Ratio,
    /// Occupancy at or below which he cuts the rent. Below `raise_at`.
    pub cut_at: Ratio,
    /// The lowest rent he sets, as a share of `reference_rent`. At most `max_rent`.
    pub min_rent: Ratio,
    /// The highest rent he sets, as a share of `reference_rent`.
    pub max_rent: Ratio,
    /// Buys the city's apartment blocks at auction when he can (R28).
    pub buy_city_blocks: bool,
    /// From this time to midnight, a negative balance makes him sell a block to the city (R5).
    pub rescue_after: ClockTime,
}

/// An AI personality.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Personality {
    /// Display name.
    pub name: String,
    /// One line on how it plays.
    pub description: String,
    /// Evaluation weights.
    pub weights: PersonalityWeights,
}

/// Weights for the shared evaluation function.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PersonalityWeights {
    /// Appetite for land leases.
    pub land: Ratio,
    /// Appetite for retail businesses.
    pub retail: Ratio,
    /// Appetite for residential.
    pub residential: Ratio,
    /// Willingness to bid high and run an overdraft.
    pub risk: Ratio,
    /// Preference for low-tier blocks.
    pub tier_low: Ratio,
    /// Preference for medium-tier blocks.
    pub tier_medium: Ratio,
    /// Preference for high-tier blocks.
    pub tier_high: Ratio,
}

/// When a personality leaves its home line of business for another that is earning more (R66):
/// the switch rule, the cap on how far it strays, and how it builds and lets apartments when it
/// does. The yields it compares are `ai::kit::line_yields`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpportunismConfig {
    /// Whether it ever leaves its home line.
    pub enabled: bool,
    /// How much more the other line must earn per pound tied up before it moves in: `0.25`
    /// means 25% more than its own line's daily yield. Between 0 and 10.
    pub threshold: Ratio,
    /// The most of its holdings (build cost of its buildings, plus its leases' book value for a
    /// land dealer) it lets sit outside its home line. Between 0 and 1.
    pub max_outside_share: Ratio,
    /// Footprint of each apartment block it builds, `[width, depth]` in lot units.
    pub apartment_footprint: [u32; 2],
    /// Height of each apartment block; capped at the type's `max_height` (R58).
    #[schemars(range(min = 1, max = 100))]
    pub apartment_height: u32,
    /// Apartment blocks it builds per lease at most (0: it never builds, it only buys).
    #[schemars(range(max = 81))]
    pub apartments_per_lot: u32,
    /// Fit-out of the blocks it builds (§7.1).
    pub fit_out: FitOutLevel,
    /// A new block's rent, as a share of `residential.reference_rent` (R51).
    pub start_rent: Ratio,
    /// How far it moves the rent each morning, as a share of `reference_rent`. Above 0.
    pub rent_step: Ratio,
    /// Occupancy at or above which it raises the rent. At most 1.
    pub raise_at: Ratio,
    /// Occupancy at or below which it cuts the rent. Below `raise_at`.
    pub cut_at: Ratio,
    /// The lowest rent it sets, as a share of `reference_rent`. At most `max_rent`.
    pub min_rent: Ratio,
    /// The highest rent it sets, as a share of `reference_rent`.
    pub max_rent: Ratio,
}
