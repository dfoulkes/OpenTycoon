//! Utilities: electricity, gas, water, telecoms→internet; bills, holders, nationalisation
//! (§6; R11, R12, R24). Post-v1 for ownership, but every building pays bills from M1.
//!
//! Owner boundary: the per-building utility bill and who it is paid to. Elections that move
//! ownership are raised by [`crate::events`].
//!
//! Status: types and the bill formula are real.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::economy::AccountId;
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
}

/// Who collects each utility's revenue. Absent means the city (§6).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Utilities {
    /// Holder per utility.
    pub holders: BTreeMap<UtilityKind, AccountId>,
}

impl Utilities {
    /// Who is paid for `kind`.
    pub fn payee(&self, kind: UtilityKind) -> AccountId {
        self.holders.get(&kind).copied().unwrap_or(AccountId::City)
    }
}

/// One utility's daily bill for a building: `volume × base_rate × business_factor` (§5.3).
/// A holder's own buildings still compute a bill; it nets to zero because payer = payee (R24).
pub fn utility_bill(volume: i64, base_rate: Money, business_factor: Rate) -> Money {
    business_factor.apply(base_rate.saturating_mul(volume))
}
