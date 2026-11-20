//! Economy: accounts, the ledger, overdraft, settlement, solvency/bankruptcy, net worth
//! (§2.3, §3; R3, R4, R5 sell-back, R7, R19, R31, R32, R39).
//!
//! Owner boundary: every movement of money goes through [`Ledger::transfer`], which conserves
//! the total. Other modules compute *what* is owed (rent, bills, stock) and hand it here as
//! [`Bill`]s; they never touch balances directly.
//!
//! Submodules:
//! - [`settlement`]: the 06:00 bills (rent, utilities, stock orders, overdraft interest; R32).
//! - [`net_worth`]: the win metric, recalculated every midnight before the write-off (R19,
//!   R31, R39).
//! - [`bankruptcy`]: the hand-over to the city (R4) and the instant sell-back (R5).

pub mod bankruptcy;
pub mod net_worth;
pub mod settlement;

/// Unit-test helper: the shipped config, embedded at compile time (no runtime I/O).
#[cfg(test)]
pub(crate) mod test_support {
    use crate::config::{Config, ConfigSources};

    macro_rules! shipped {
        ($($file:literal),* $(,)?) => {
            ConfigSources::new()
                $(.with($file, include_str!(concat!("../../../../config/", $file))))*
        };
    }

    /// The repo's shipped `config/`, loaded and validated.
    #[allow(clippy::expect_used, reason = "test helper")]
    pub(crate) fn shipped_config() -> Config {
        let sources = shipped!(
            "game.yaml",
            "map.yaml",
            "auctions.yaml",
            "utilities.yaml",
            "businesses.yaml",
            "customers.yaml",
            "events.yaml",
            "ai.yaml",
        );
        Config::load(&sources).expect("shipped config is valid")
    }

    /// The shipped config on the grey-box 3×3 map (`map: v1`), for tests about it.
    #[allow(clippy::expect_used, reason = "test helper")]
    pub(crate) fn shipped_config_v1() -> Config {
        let mut sources = shipped!(
            "game.yaml",
            "auctions.yaml",
            "utilities.yaml",
            "businesses.yaml",
            "customers.yaml",
            "events.yaml",
            "ai.yaml",
        );
        sources.insert(
            "map.yaml",
            include_str!("../../../../config/map.yaml").replacen("\nmap: full\n", "\nmap: v1\n", 1),
        );
        Config::load(&sources).expect("shipped config is valid")
    }
}

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::config::game::OverdraftSettings;
use crate::money::Money;

pub use net_worth::{HeldLot, NetWorthBoard, NetWorthInputs, net_worth};

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
    /// The shoppers' purse: every shopper's allowance, minted each morning (R63). Purchases
    /// move money from here to the shop's owner. Not a player and never checked for solvency.
    Shoppers,
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
    /// Money created since setup (R63).
    #[serde(default)]
    minted: Money,
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

    /// Creates `amount` of new money in `account` (R63: shoppers' daily allowances). The only
    /// way the ledger total grows after setup; [`Ledger::minted`] counts it.
    pub fn mint(&mut self, account: AccountId, amount: Money) {
        *self.balances.entry(account).or_default() += amount;
        self.minted += amount;
    }

    /// All money minted since the game began (R63).
    pub fn minted(&self) -> Money {
        self.minted
    }

    /// Moves `amount` from one account to another. No limit check: callers that must respect
    /// the overdraft (bids, purchases) check [`spendable`] first; settlement deliberately
    /// does not (R32).
    pub fn transfer(&mut self, from: AccountId, to: AccountId, amount: Money) {
        *self.balances.entry(from).or_default() -= amount;
        *self.balances.entry(to).or_default() += amount;
    }

    /// Moves `amount` only if the payer stays within its overdraft: a player's balance after
    /// paying may not fall below `−credit_limit` (R7). The city has no limit (R4). Use this for
    /// anything a player chooses to buy (a winning bid, a building); settlement uses
    /// [`Ledger::settle`], which has no limit (R32).
    pub fn pay_within_limit(
        &mut self,
        from: AccountId,
        to: AccountId,
        amount: Money,
        credit_limit: Money,
    ) -> Result<(), Overdrawn> {
        if let AccountId::Player(_) = from {
            let ceiling = spendable(self.balance(from), credit_limit);
            if amount > ceiling {
                return Err(Overdrawn { amount, ceiling });
            }
        }
        self.transfer(from, to, amount);
        Ok(())
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

/// A payment refused because it would take the payer past its credit limit (R7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{amount} is more than the {ceiling} this player can spend (balance + credit limit)")]
pub struct Overdrawn {
    /// What the payment was for.
    pub amount: Money,
    /// The most the payer could spend: balance + credit limit.
    pub ceiling: Money,
}

/// Credit limit after `steps` year steps: `start + steps × increase` (R7).
pub fn credit_limit(start_limit: Money, increase_per_step: Money, steps: u32) -> Money {
    start_limit + increase_per_step.saturating_mul(i64::from(steps))
}

/// Credit limit on game day `day` (day 0 is the first day; each midnight is one 5-year step)
/// from `game.yaml`'s overdraft settings (R7).
pub fn credit_limit_on_day(overdraft: &OverdraftSettings, day: u32) -> Money {
    credit_limit(
        overdraft.start_limit.money(),
        overdraft.increase_per_step.money(),
        day,
    )
}

/// How much a player can spend or bid: `balance + credit_limit` (R7). This is the bid ceiling
/// of §5.1; it is negative when a player is already past the limit (R32).
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
    /// Consecutive failed midnights so far (0 after a passed midnight).
    pub fn strikes(&self, player: PlayerId) -> u32 {
        self.strikes.get(&player).copied().unwrap_or(0)
    }

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
