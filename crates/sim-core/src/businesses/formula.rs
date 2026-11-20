//! The business-stats formula (docs/economy.md "Business stats"; §7.1, §7.4; R10, R35).
//!
//! Stub: the scenarios in `features/businesses/eras.feature` are written against it first.

use crate::config::Config;
use crate::config::businesses::BusinessType;
use crate::land::Tier;
use crate::money::Money;

/// Height of the reference building (`reference.height`).
pub fn reference_height(_config: &Config) -> u32 {
    1
}

/// Units of each good a reference shop of `kind` sells in a day: its reference footfall.
pub fn reference_footfall(_kind: &BusinessType, _config: &Config) -> i64 {
    0
}

/// What a reference shop of `kind` makes in a day at reference footfall, on `tier` in `year`,
/// after rent, utilities and stock. `None` for a type with no goods.
pub fn reference_daily_profit(
    _kind: &BusinessType,
    _config: &Config,
    _tier: Tier,
    _year: i32,
) -> Option<Money> {
    Some(Money::ZERO)
}
