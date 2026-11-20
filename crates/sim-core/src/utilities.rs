//! Utilities: electricity, gas, water, telecoms→internet; bills, holders, nationalisation
//! (§6; R11, R12, R24).
//!
//! Owner boundary: the per-building utility bill, who it is paid to, and who holds each
//! utility. Elections that move ownership are decided by [`crate::events`]; the world applies
//! them here.
//!
//! - **Four utilities from 1930** (R11): electricity, gas, water and the telephone exchange,
//!   which becomes the internet in `internet.upgrade_year` (1995). Every building pays every
//!   utility's bill at 06:00; from 1995 the telecoms bill is the internet bill.
//! - **Auctioned like land** (R12, R24): a player opens an auction on an unheld utility with
//!   an opening bid of at least its `reserve_prices` entry; the same queue and rules as land.
//!   The winner holds it until they offer it for auction, go bankrupt (R4), or Labour
//!   nationalises it. There is no lease term.
//! - **Revenue** (§6): the holder collects that utility's bills map-wide; their own bills net
//!   to zero (R24). Unheld and nationalised utilities pay the city.
//! - **Nationalisation** (§9.1): every utility passes to the state; each holder is bought out
//!   by the city at **book value × `buyout_ratio`**. Book value is the price the holder paid at
//!   auction: utilities don't wear (as R41 rules for buildings) and have no other asset value
//!   (R19). A nationalised utility is not open for bids.
//! - **Privatisation** (§9.1): every nationalised utility is put up for auction by the city,
//!   through the auction house's queue, reserve as above.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::economy::{AccountId, Bill};
use crate::money::{Money, Rate};

/// A utility (§6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum UtilityKind {
    /// Electricity.
    Electricity,
    /// Gas.
    Gas,
    /// Water.
    Water,
    /// Telephone exchange; the internet from the upgrade year (R11).
    Telecoms,
}

impl UtilityKind {
    /// All four, in a fixed order.
    pub const ALL: [UtilityKind; 4] = [
        UtilityKind::Electricity,
        UtilityKind::Gas,
        UtilityKind::Water,
        UtilityKind::Telecoms,
    ];

    /// The utility's name in `year`: the telephone exchange is the internet from the upgrade
    /// year (R11).
    pub fn name(self, year: i32, upgrade_year: i32) -> &'static str {
        match self {
            UtilityKind::Electricity => "Electricity",
            UtilityKind::Gas => "Gas",
            UtilityKind::Water => "Water",
            UtilityKind::Telecoms if year >= upgrade_year => "Internet",
            UtilityKind::Telecoms => "Telephone Exchange",
        }
    }

    /// Looks a utility up by any of its names, case-insensitively.
    pub fn from_name(name: &str) -> Option<UtilityKind> {
        match name.to_ascii_lowercase().as_str() {
            "electricity" => Some(UtilityKind::Electricity),
            "gas" => Some(UtilityKind::Gas),
            "water" => Some(UtilityKind::Water),
            "telecoms" | "telephone exchange" | "internet" => Some(UtilityKind::Telecoms),
            _ => None,
        }
    }
}

/// Who collects each utility's revenue (§6). Absent from `holders` means the city.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Utilities {
    /// Holder per utility.
    pub holders: BTreeMap<UtilityKind, AccountId>,
    /// What each holder paid at auction: the book value a buy-out pays (§9.1).
    #[serde(default)]
    pub book_values: BTreeMap<UtilityKind, Money>,
    /// Utilities in state hands since a Labour win; not open for bids (§9.1).
    #[serde(default)]
    pub nationalised: BTreeSet<UtilityKind>,
}

impl Utilities {
    /// Who is paid for `kind`.
    pub fn payee(&self, kind: UtilityKind) -> AccountId {
        self.holders.get(&kind).copied().unwrap_or(AccountId::City)
    }

    /// True if a player may open an auction on `kind` (R12): nobody holds it and it is not
    /// nationalised.
    pub fn is_open_for_bids(&self, kind: UtilityKind) -> bool {
        !self.holders.contains_key(&kind) && !self.nationalised.contains(&kind)
    }

    /// The price `kind`'s holder paid; zero if unheld.
    pub fn book_value(&self, kind: UtilityKind) -> Money {
        self.book_values.get(&kind).copied().unwrap_or_default()
    }

    /// Records an auction winner (R12). The only way a player comes to hold a utility; the
    /// world calls it when a utility auction closes and the winner has paid.
    pub fn grant(&mut self, kind: UtilityKind, holder: AccountId, price: Money) {
        if holder == AccountId::City {
            self.holders.remove(&kind);
            self.book_values.remove(&kind);
        } else {
            self.holders.insert(kind, holder);
            self.book_values.insert(kind, price);
        }
    }

    /// Hands every utility `account` holds back to the city, unheld and open for bids (R4).
    /// Returns them, in order.
    pub fn release(&mut self, account: AccountId) -> Vec<UtilityKind> {
        let mine: Vec<UtilityKind> = self
            .holders
            .iter()
            .filter(|(_, h)| **h == account)
            .map(|(k, _)| *k)
            .collect();
        for kind in &mine {
            self.holders.remove(kind);
            self.book_values.remove(kind);
        }
        mine
    }

    /// A Labour win (§9.1): every utility is nationalised and each holder is bought out by the
    /// city at `book value × buyout_ratio`. Returns the buy-out payments for the world to move
    /// through the ledger, in utility order.
    pub fn nationalise(&mut self, buyout_ratio: Rate) -> Vec<Bill> {
        let buyouts = buyout_bills(self, buyout_ratio);
        self.holders.clear();
        self.book_values.clear();
        self.nationalised = UtilityKind::ALL.into_iter().collect();
        buyouts
    }

    /// A Conservative win (§9.1): every nationalised utility leaves state hands and is
    /// returned, in order, for the world to put up for auction.
    pub fn privatise(&mut self) -> Vec<UtilityKind> {
        std::mem::take(&mut self.nationalised).into_iter().collect()
    }
}

/// The city's buy-out of each held utility: `book value × buyout_ratio` from the city to the
/// holder (§9.1). Zero payments are left out.
pub fn buyout_bills(utilities: &Utilities, buyout_ratio: Rate) -> Vec<Bill> {
    utilities
        .holders
        .iter()
        .map(|(kind, holder)| Bill {
            from: AccountId::City,
            to: *holder,
            amount: buyout_ratio.apply(utilities.book_value(*kind)),
        })
        .filter(|b| b.amount > Money::ZERO)
        .collect()
}

/// One utility's daily bill for a building: `volume × base_rate × business_factor` (§5.3).
/// A holder's own buildings still compute a bill; it nets to zero because payer = payee (R24).
pub fn utility_bill(volume: i64, base_rate: Money, business_factor: Rate) -> Money {
    business_factor.apply(base_rate.saturating_mul(volume))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economy::{Ledger, PlayerId};

    const P0: AccountId = AccountId::Player(PlayerId(0));
    const P1: AccountId = AccountId::Player(PlayerId(1));

    #[test]
    fn nationalisation_buys_holders_out_at_book_value_and_closes_bidding() {
        let mut u = Utilities::default();
        u.grant(UtilityKind::Electricity, P0, Money::from_pounds(3_000));
        u.grant(UtilityKind::Water, P1, Money::from_pounds(2_100));
        let bills = u.nationalise(Rate::ONE);
        assert_eq!(
            bills,
            vec![
                Bill {
                    from: AccountId::City,
                    to: P0,
                    amount: Money::from_pounds(3_000)
                },
                Bill {
                    from: AccountId::City,
                    to: P1,
                    amount: Money::from_pounds(2_100)
                },
            ]
        );
        assert!(
            UtilityKind::ALL
                .iter()
                .all(|k| u.payee(*k) == AccountId::City)
        );
        assert!(UtilityKind::ALL.iter().all(|k| !u.is_open_for_bids(*k)));
        assert_eq!(u.privatise(), UtilityKind::ALL.to_vec());
        assert!(UtilityKind::ALL.iter().all(|k| u.is_open_for_bids(*k)));
    }

    #[test]
    fn the_buyout_ratio_scales_book_value() {
        let mut u = Utilities::default();
        u.grant(UtilityKind::Gas, P0, Money::from_pounds(2_000));
        let bills = u.nationalise(Rate::from_bp(5_000));
        assert_eq!(bills[0].amount, Money::from_pounds(1_000));
    }

    #[test]
    fn bankruptcy_releases_a_players_utilities_to_the_city() {
        let mut u = Utilities::default();
        u.grant(UtilityKind::Gas, P0, Money::from_pounds(2_000));
        u.grant(UtilityKind::Water, P1, Money::from_pounds(2_000));
        assert_eq!(u.release(P0), vec![UtilityKind::Gas]);
        assert_eq!(u.payee(UtilityKind::Gas), AccountId::City);
        assert!(u.is_open_for_bids(UtilityKind::Gas));
        assert_eq!(u.payee(UtilityKind::Water), P1);
    }

    #[test]
    fn the_exchange_is_the_internet_from_the_upgrade_year() {
        assert_eq!(UtilityKind::Telecoms.name(1990, 1995), "Telephone Exchange");
        assert_eq!(UtilityKind::Telecoms.name(1995, 1995), "Internet");
        assert_eq!(
            UtilityKind::from_name("Internet"),
            Some(UtilityKind::Telecoms)
        );
    }

    proptest::proptest! {
        /// Buy-outs move money from the city to holders and create none.
        #[test]
        fn buyouts_conserve_money(prices in proptest::collection::vec(0i64..10_000_000, 4), ratio in 0i64..20_000) {
            let mut u = Utilities::default();
            for (i, kind) in UtilityKind::ALL.into_iter().enumerate() {
                u.grant(kind, if i % 2 == 0 { P0 } else { P1 }, Money::from_pence(prices[i]));
            }
            let mut ledger = Ledger::new();
            ledger.open(AccountId::City, Money::ZERO);
            let before = ledger.total();
            ledger.settle(&u.nationalise(Rate::from_bp(ratio)));
            proptest::prop_assert_eq!(ledger.total(), before);
        }
    }
}
