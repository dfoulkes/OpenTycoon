//! Money and rates. All money is integer pence; nothing in the sim uses floats for money.
//!
//! - [`Money`]: signed pence (`i64`). Balances may go negative (overdraft, R7/R32).
//! - [`Rate`]: a fixed-point multiplier in basis points (1/10,000), used for ratios such as
//!   `sell_to_city_ratio` (R5) or `desirability_mult` (§5.3). Config gives these as decimals;
//!   the loader converts them once, so the sim only ever does integer maths.
//!
//! Rounding rule (applies everywhere): [`Rate::apply`] rounds half away from zero.

use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Neg, Sub, SubAssign};

use serde::{Deserialize, Serialize};

/// Pence in a pound.
pub const PENCE_PER_POUND: i64 = 100;

/// An amount of money in integer pence. Negative means owed.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
#[must_use]
pub struct Money(i64);

impl Money {
    /// Zero pence.
    pub const ZERO: Money = Money(0);

    /// Builds an amount from pence.
    pub const fn from_pence(pence: i64) -> Self {
        Money(pence)
    }

    /// Builds an amount from whole pounds.
    pub const fn from_pounds(pounds: i64) -> Self {
        Money(pounds * PENCE_PER_POUND)
    }

    /// The amount in pence.
    pub const fn pence(self) -> i64 {
        self.0
    }

    /// True when the amount is below zero.
    pub const fn is_negative(self) -> bool {
        self.0 < 0
    }

    /// Adds, returning `None` on overflow.
    pub fn checked_add(self, rhs: Money) -> Option<Money> {
        self.0.checked_add(rhs.0).map(Money)
    }

    /// Subtracts, returning `None` on overflow.
    pub fn checked_sub(self, rhs: Money) -> Option<Money> {
        self.0.checked_sub(rhs.0).map(Money)
    }

    /// Multiplies by an integer count (units, volume), returning `None` on overflow.
    pub fn checked_mul(self, count: i64) -> Option<Money> {
        self.0.checked_mul(count).map(Money)
    }

    /// Multiplies by an integer count, saturating at the `i64` bounds.
    pub fn saturating_mul(self, count: i64) -> Money {
        Money(self.0.saturating_mul(count))
    }
}

impl Add for Money {
    type Output = Money;
    fn add(self, rhs: Money) -> Money {
        Money(self.0 + rhs.0)
    }
}

impl AddAssign for Money {
    fn add_assign(&mut self, rhs: Money) {
        self.0 += rhs.0;
    }
}

impl Sub for Money {
    type Output = Money;
    fn sub(self, rhs: Money) -> Money {
        Money(self.0 - rhs.0)
    }
}

impl SubAssign for Money {
    fn sub_assign(&mut self, rhs: Money) {
        self.0 -= rhs.0;
    }
}

impl Neg for Money {
    type Output = Money;
    fn neg(self) -> Money {
        Money(-self.0)
    }
}

impl Sum for Money {
    fn sum<I: Iterator<Item = Money>>(iter: I) -> Money {
        iter.fold(Money::ZERO, Add::add)
    }
}

/// Displays as decimal pounds, e.g. `£1,234.05` or `-£0.50`. £sd display (pre-1971) is a
/// client concern, not the sim's.
impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let abs = self.0.unsigned_abs();
        let pounds = abs / 100;
        let pence = abs % 100;
        let digits = pounds.to_string();
        let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
        for (i, ch) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i) % 3 == 0 {
                grouped.push(',');
            }
            grouped.push(ch);
        }
        write!(f, "{sign}£{grouped}.{pence:02}")
    }
}

/// Basis points in 1.0.
pub const BASIS_POINTS_PER_UNIT: i64 = 10_000;

/// A fixed-point multiplier in basis points: `Rate::from_bp(5_000)` is ×0.5.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
#[must_use]
pub struct Rate(i64);

impl Rate {
    /// ×1.0
    pub const ONE: Rate = Rate(BASIS_POINTS_PER_UNIT);
    /// ×0.0
    pub const ZERO: Rate = Rate(0);

    /// Builds a rate from basis points (1/10,000).
    pub const fn from_bp(bp: i64) -> Self {
        Rate(bp)
    }

    /// The rate in basis points.
    pub const fn bp(self) -> i64 {
        self.0
    }

    /// Applies the rate to an amount, rounding half away from zero.
    ///
    /// Uses `i128` internally so no realistic amount can overflow.
    pub fn apply(self, amount: Money) -> Money {
        let product = i128::from(amount.pence()) * i128::from(self.0);
        let denom = i128::from(BASIS_POINTS_PER_UNIT);
        let half = denom / 2;
        let rounded = if product >= 0 {
            (product + half) / denom
        } else {
            (product - half) / denom
        };
        // Clamp rather than panic: amounts this large are already a bug elsewhere.
        Money::from_pence(i64::try_from(rounded).unwrap_or(if rounded > 0 {
            i64::MAX
        } else {
            i64::MIN
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn displays_pounds_with_grouping() {
        assert_eq!(Money::from_pence(123_405).to_string(), "£1,234.05");
        assert_eq!(Money::from_pence(-50).to_string(), "-£0.50");
        assert_eq!(Money::ZERO.to_string(), "£0.00");
        assert_eq!(Money::from_pounds(1_000_000).to_string(), "£1,000,000.00");
    }

    #[test]
    fn rate_rounds_half_away_from_zero() {
        let half = Rate::from_bp(5_000);
        assert_eq!(half.apply(Money::from_pence(3)), Money::from_pence(2));
        assert_eq!(half.apply(Money::from_pence(-3)), Money::from_pence(-2));
        assert_eq!(Rate::ONE.apply(Money::from_pence(7)), Money::from_pence(7));
    }

    proptest! {
        #[test]
        fn add_then_sub_is_identity(a in -1_000_000_000i64..1_000_000_000, b in -1_000_000_000i64..1_000_000_000) {
            let (a, b) = (Money::from_pence(a), Money::from_pence(b));
            prop_assert_eq!(a + b - b, a);
        }

        #[test]
        fn rate_one_is_identity(p in -1_000_000_000_000i64..1_000_000_000_000) {
            prop_assert_eq!(Rate::ONE.apply(Money::from_pence(p)), Money::from_pence(p));
        }

        #[test]
        fn rate_is_symmetric_in_sign(p in 0i64..1_000_000_000, bp in 0i64..100_000) {
            let r = Rate::from_bp(bp);
            prop_assert_eq!(r.apply(Money::from_pence(-p)), -r.apply(Money::from_pence(p)));
        }
    }
}
