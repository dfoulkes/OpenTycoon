//! Economy: accounts, the ledger, overdraft, settlement, solvency/bankruptcy, net worth
//! (§2.3, §3; R3, R4, R7, R19, R31, R32, R39).
//!
//! Owner boundary: every movement of money goes through [`Ledger::transfer`], which conserves
//! the total. Other modules compute *what* is owed (rent, bills, stock) and hand it here as
//! [`Bill`]s; they never touch balances directly.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::money::{Money, Rate};

/// A player seat. `0` is the first seat; seats are stable for the life of a game.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
pub struct PlayerId(pub u8);

/// Who holds money or assets. The city is an account that can't go bust (R4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum AccountId {
    /// The city (City Hall).
    City,
    /// A player.
    Player(PlayerId),
}

/// A sum owed from one account to another, settled at 06:00 (R32: may push past the limit).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Bill {
    /// Payer.
    pub from: AccountId,
    /// Payee.
    pub to: AccountId,
    /// Amount (non-negative).
    pub amount: Money,
}

/// Balances per account. Money only moves by transfer, so the total is conserved.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Ledger {
    balances: BTreeMap<AccountId, Money>,
}

impl Ledger {
    /// An empty ledger.
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens an account with a starting balance. This is the only way money enters the game,
    /// and only at setup or when population brings spending money onto the map.
    pub fn open(&mut self, account: AccountId, opening_balance: Money) {
        self.balances.insert(account, opening_balance);
    }

    /// An account's balance (zero if never opened).
    pub fn balance(&self, account: AccountId) -> Money {
        self.balances.get(&account).copied().unwrap_or_default()
    }

    /// Sum of every balance.
    pub fn total(&self) -> Money {
        self.balances.values().copied().sum()
    }

    /// Moves `amount` from one account to another. No limit check: callers that must respect
    /// the overdraft (bids, purchases) check [`spendable`] first; settlement deliberately
    /// does not (R32).
    pub fn transfer(&mut self, from: AccountId, to: AccountId, amount: Money) {
        *self.balances.entry(from).or_default() -= amount;
        *self.balances.entry(to).or_default() += amount;
    }

    /// Pays every bill in order (06:00 settlement). Balances may go below the limit (R32).
    pub fn settle(&mut self, bills: &[Bill]) {
        for bill in bills {
            self.transfer(bill.from, bill.to, bill.amount);
        }
    }

    /// Accounts in id order.
    pub fn accounts(&self) -> impl Iterator<Item = (AccountId, Money)> + '_ {
        self.balances.iter().map(|(a, m)| (*a, *m))
    }
}

/// Credit limit after `steps` year steps: `start + steps × increase` (R7).
pub fn credit_limit(start_limit: Money, increase_per_step: Money, steps: u32) -> Money {
    start_limit + increase_per_step.saturating_mul(i64::from(steps))
}

/// How much a player can spend or bid: `balance + credit_limit` (R7).
pub fn spendable(balance: Money, credit_limit: Money) -> Money {
    balance + credit_limit
}

/// Result of a player's midnight solvency check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Solvency {
    /// Balance at or above the threshold (R3: exactly £0 passes). Strikes reset.
    Passed,
    /// Failed; the player now has this many consecutive strikes.
    Strike(u32),
    /// Failed enough midnights in a row: out of the game, assets to the city (R4).
    Bankrupt,
}

/// Consecutive failed midnights per player (§2.3). Only a passed midnight resets the count.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct SolvencyTracker {
    strikes: BTreeMap<PlayerId, u32>,
}

impl SolvencyTracker {
    /// Checks one player's midnight balance.
    pub fn check(
        &mut self,
        player: PlayerId,
        balance: Money,
        solvent_at_or_above: Money,
        strikes_to_bankrupt: u32,
    ) -> Solvency {
        if balance >= solvent_at_or_above {
            self.strikes.remove(&player);
            return Solvency::Passed;
        }
        let strikes = self.strikes.entry(player).or_insert(0);
        *strikes += 1;
        if *strikes >= strikes_to_bankrupt {
            Solvency::Bankrupt
        } else {
            Solvency::Strike(*strikes)
        }
    }
}

/// The parts of one player's net worth (R19, R39), all already valued.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetWorthInputs {
    /// Cash balance.
    pub cash: Money,
    /// Stock on hand at purchase price (before the midnight write-off, R31).
    pub stock: Money,
    /// Value of each property the player owns (build cost, R41).
    pub owned_property_values: Vec<Money>,
    /// For each lot the player leases: its land value and the values of every property on it,
    /// whoever owns them.
    pub held_lots: Vec<HeldLot>,
}

/// A leased lot's contribution to net worth.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeldLot {
    /// `last auction price × remaining lease years / term` (§3).
    pub land_value: Money,
    /// Value of every property on the lot.
    pub property_values: Vec<Money>,
}

/// `net_worth = cash + stock + Σ owned property + Σ held lots (land + uplift × Σ property)`
/// (§3, R19, R39). Utilities add nothing directly (R19).
pub fn net_worth(inputs: &NetWorthInputs, lot_property_uplift: Rate) -> Money {
    let owned: Money = inputs.owned_property_values.iter().copied().sum();
    let lots: Money = inputs
        .held_lots
        .iter()
        .map(|lot| {
            let on_lot: Money = lot.property_values.iter().copied().sum();
            lot.land_value + lot_property_uplift.apply(on_lot)
        })
        .sum();
    inputs.cash + inputs.stock + owned + lots
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const P0: AccountId = AccountId::Player(PlayerId(0));
    const P1: AccountId = AccountId::Player(PlayerId(1));

    #[test]
    fn credit_limit_grows_flat_per_step() {
        let limit = |s| credit_limit(Money::from_pounds(10_000), Money::from_pounds(2_500), s);
        assert_eq!(limit(0), Money::from_pounds(10_000));
        assert_eq!(limit(24), Money::from_pounds(70_000));
    }

    #[test]
    fn zero_balance_is_solvent_and_two_strikes_bankrupt() {
        let mut t = SolvencyTracker::default();
        let p = PlayerId(0);
        assert_eq!(t.check(p, Money::ZERO, Money::ZERO, 2), Solvency::Passed);
        assert_eq!(
            t.check(p, Money::from_pence(-1), Money::ZERO, 2),
            Solvency::Strike(1)
        );
        assert_eq!(
            t.check(p, Money::from_pence(-1), Money::ZERO, 2),
            Solvency::Bankrupt
        );
    }

    #[test]
    fn a_passed_midnight_resets_strikes() {
        let mut t = SolvencyTracker::default();
        let p = PlayerId(0);
        t.check(p, Money::from_pence(-1), Money::ZERO, 2);
        t.check(p, Money::ZERO, Money::ZERO, 2);
        assert_eq!(
            t.check(p, Money::from_pence(-1), Money::ZERO, 2),
            Solvency::Strike(1)
        );
    }

    #[test]
    fn owning_land_and_building_earns_the_uplift() {
        let inputs = NetWorthInputs {
            cash: Money::from_pounds(100),
            stock: Money::from_pounds(10),
            owned_property_values: vec![Money::from_pounds(1_000)],
            held_lots: vec![HeldLot {
                land_value: Money::from_pounds(500),
                property_values: vec![Money::from_pounds(1_000)],
            }],
        };
        // 100 + 10 + 1000 + 500 + 0.2 × 1000
        assert_eq!(
            net_worth(&inputs, Rate::from_bp(2_000)),
            Money::from_pounds(1_810)
        );
    }

    proptest! {
        #[test]
        fn transfers_conserve_money(moves in prop::collection::vec((0u8..3, 0u8..3, -1_000_000i64..1_000_000), 0..200)) {
            let mut ledger = Ledger::new();
            ledger.open(AccountId::City, Money::from_pounds(1_000_000));
            ledger.open(P0, Money::from_pounds(5_000));
            ledger.open(P1, Money::from_pounds(5_000));
            let before = ledger.total();
            let acct = |i: u8| match i { 0 => AccountId::City, 1 => P0, _ => P1 };
            for (from, to, pence) in moves {
                ledger.transfer(acct(from), acct(to), Money::from_pence(pence));
            }
            prop_assert_eq!(ledger.total(), before);
        }
    }
}
