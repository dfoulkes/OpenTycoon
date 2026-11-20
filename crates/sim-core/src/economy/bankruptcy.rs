//! Bankruptcy hand-over (R4) and the instant sell-back to the city (R5, the economy half).
//!
//! - **Bankruptcy** (§2.3): a bankrupt player's leases go back to the city (the block becomes
//!   unleased, so it pays rent to the city and can be auctioned as land only), and their
//!   buildings, with whatever stock is in them, become city-owned. The city runs those
//!   businesses at default settings. The city has no solvency check, so it can't go bust.
//! - **Sell-back** (§2.3): one click, instant, for `sell_to_city_ratio` (default 50%) of the
//!   true value, which is the property value, i.e. the build cost (§3, R41). The land is not
//!   part of the sale. The business-auction half of R5 belongs to the auctions module.
//!
//! "Default settings" are not spelled out by the spec. Until the businesses module owns them,
//! [`city_defaults`] sets every good to its reference price (100%) and orders the building's
//! full stock capacity, split evenly across its goods (§7.1: multi-good stores split their
//! capacity).

use super::{AccountId, Ledger, PlayerId};
use crate::businesses::{Business, BusinessId, Businesses};
use crate::config::Config;
use crate::config::businesses::BusinessType;
use crate::land::{BlockId, Map};
use crate::money::{Money, Rate};

/// What a bankrupt player hands to the city (R4).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Handover {
    /// Blocks whose leases go back to the city.
    pub leases: Vec<BlockId>,
    /// Businesses (buildings and their stock) that become city-owned.
    pub businesses: Vec<BusinessId>,
}

/// Everything `account` holds: its leases and its businesses, in id order.
pub fn assets_of(account: AccountId, map: &Map, businesses: &Businesses) -> Handover {
    Handover {
        leases: map
            .blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| b.lease.as_ref().is_some_and(|l| l.holder == account))
            .filter_map(|(i, _)| u32::try_from(i).ok().map(BlockId))
            .collect(),
        businesses: businesses
            .by_id
            .iter()
            .filter(|(_, b)| b.owner == account)
            .map(|(id, _)| *id)
            .collect(),
    }
}

/// Hands a bankrupt player's assets to the city (R4): leases end (the land is unleased and
/// pays rent to the city), buildings and their stock become city-owned and run at defaults.
pub fn hand_over_to_city(
    handover: &Handover,
    map: &mut Map,
    businesses: &mut Businesses,
    config: &Config,
) {
    for block in &handover.leases {
        if let Some(b) = map.blocks.get_mut(block.0 as usize) {
            b.lease = None;
        }
    }
    for id in &handover.businesses {
        if let Some(business) = businesses.by_id.get_mut(id) {
            take_into_city_hands(business, config);
        }
    }
}

/// The city takes a business over and runs it at default settings.
fn take_into_city_hands(business: &mut Business, config: &Config) {
    business.owner = AccountId::City;
    if let Some(business_type) = config.businesses.types.get(&business.type_id) {
        city_defaults(business, business_type);
    }
}

/// Default settings for a city-run business (R4): reference prices, an order for its full
/// stock capacity split evenly across its goods, open for trade. Owned by businesses
/// ([`crate::businesses::city_defaults`]); kept here for the hand-over's call sites.
pub fn city_defaults(business: &mut Business, business_type: &BusinessType) {
    crate::businesses::city_defaults(business, business_type);
}

/// The price the city pays for a business: `sell_to_city_ratio × build cost` (R5, R41).
pub fn sell_back_price(build_cost: Money, sell_to_city_ratio: Rate) -> Money {
    sell_to_city_ratio.apply(build_cost)
}

/// Why a sell-back was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SellBackError {
    /// No such business.
    #[error("no such business")]
    NoSuchBusiness,
    /// The seller does not own it.
    #[error("the seller does not own this business")]
    NotOwner,
}

/// Sells `business` to the city at once (R5): the city pays `sell_to_city_ratio × build cost`
/// and runs it at defaults. Returns the price paid. The lease on the land is untouched.
pub fn sell_to_city(
    seller: PlayerId,
    business: BusinessId,
    ledger: &mut Ledger,
    businesses: &mut Businesses,
    config: &Config,
) -> Result<Money, SellBackError> {
    let seller = AccountId::Player(seller);
    let b = businesses
        .by_id
        .get_mut(&business)
        .ok_or(SellBackError::NoSuchBusiness)?;
    if b.owner != seller {
        return Err(SellBackError::NotOwner);
    }
    let price = sell_back_price(
        b.build_cost,
        config.game.bankruptcy.sell_to_city_ratio.rate(),
    );
    ledger.transfer(AccountId::City, seller, price);
    take_into_city_hands(b, config);
    Ok(price)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests")]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::businesses::FitOutLevel;
    use crate::economy::test_support::shipped_config;
    use crate::land::{Footprint, Lease};
    use proptest::prelude::*;

    const P0: AccountId = AccountId::Player(PlayerId(0));
    const P1: AccountId = AccountId::Player(PlayerId(1));

    fn shop(type_id: &str, owner: AccountId, block: u32) -> Business {
        Business {
            type_id: type_id.into(),
            block: BlockId(block),
            footprint: Footprint {
                x: 0,
                y: 0,
                w: 3,
                d: 3,
            },
            height: 10,
            owner,
            fit_out: FitOutLevel::Medium,
            build_cost: Money::from_pounds(900),
            prices: [("clothes".to_owned(), Rate::from_bp(15_000))].into(),
            stock: [("clothes".to_owned(), 12)].into(),
            tomorrow_order: BTreeMap::new(),
            open: false,
            hours: crate::businesses::Hours::Day,
            capacity_per_good: 45,
            standing_order: BTreeMap::new(),
            takings_today: Money::ZERO,
            sold_today: BTreeMap::new(),
            lost_sales_today: 0,
        }
    }

    #[test]
    fn half_of_true_value_rounds_half_away_from_zero() {
        assert_eq!(
            sell_back_price(Money::from_pence(24_001), Rate::from_bp(5_000)),
            Money::from_pence(12_001)
        );
    }

    #[test]
    fn hand_over_moves_leases_buildings_and_stock_and_nothing_else() {
        let config = shipped_config();
        let mut map = Map::from_config(&config.map);
        let lease = |holder| Lease {
            holder,
            start_year: 1930,
            term_years: 30,
            last_price: Money::from_pounds(1),
        };
        map.blocks[0].lease = Some(lease(P0));
        map.blocks[1].lease = Some(lease(P1));
        let mut businesses = Businesses::default();
        businesses
            .by_id
            .insert(BusinessId(0), shop("clothes_store", P0, 3));
        businesses
            .by_id
            .insert(BusinessId(1), shop("clothes_store", P1, 3));

        let handover = assets_of(P0, &map, &businesses);
        assert_eq!(handover.leases, vec![BlockId(0)]);
        assert_eq!(handover.businesses, vec![BusinessId(0)]);
        hand_over_to_city(&handover, &mut map, &mut businesses, &config);

        assert!(map.blocks[0].lease.is_none());
        assert_eq!(map.blocks[1].lease.as_ref().unwrap().holder, P1);
        let taken = &businesses.by_id[&BusinessId(0)];
        assert_eq!(taken.owner, AccountId::City);
        assert_eq!(taken.stock["clothes"], 12, "stock goes with the building");
        assert_eq!(taken.prices["clothes"], Rate::ONE);
        assert_eq!(taken.tomorrow_order["clothes"], 27);
        assert!(taken.open);
        assert_eq!(businesses.by_id[&BusinessId(1)].owner, P1);
        assert_eq!(assets_of(P0, &map, &businesses), Handover::default());
    }

    #[test]
    fn selling_someone_elses_business_is_refused() {
        let config = shipped_config();
        let mut ledger = Ledger::new();
        let mut businesses = Businesses::default();
        businesses
            .by_id
            .insert(BusinessId(0), shop("clothes_store", P1, 3));
        assert_eq!(
            sell_to_city(
                PlayerId(0),
                BusinessId(0),
                &mut ledger,
                &mut businesses,
                &config
            ),
            Err(SellBackError::NotOwner)
        );
        assert_eq!(
            sell_to_city(
                PlayerId(0),
                BusinessId(9),
                &mut ledger,
                &mut businesses,
                &config
            ),
            Err(SellBackError::NoSuchBusiness)
        );
        assert_eq!(ledger.total(), Money::ZERO);
    }

    proptest! {
        #[test]
        fn sell_back_conserves_money_and_pays_at_most_true_value(
            cost in 0i64..100_000_000,
            ratio in 0i64..=10_000,
        ) {
            let mut config = shipped_config();
            config.game.bankruptcy.sell_to_city_ratio = crate::config::types::Ratio(Rate::from_bp(ratio));
            let mut ledger = Ledger::new();
            ledger.open(AccountId::City, Money::ZERO);
            ledger.open(P0, Money::ZERO);
            let mut businesses = Businesses::default();
            let mut b = shop("clothes_store", P0, 3);
            b.build_cost = Money::from_pence(cost);
            businesses.by_id.insert(BusinessId(0), b);
            let price = sell_to_city(PlayerId(0), BusinessId(0), &mut ledger, &mut businesses, &config).unwrap();
            prop_assert_eq!(ledger.total(), Money::ZERO);
            prop_assert!(price <= Money::from_pence(cost));
            prop_assert_eq!(ledger.balance(P0), price);
        }
    }
}
