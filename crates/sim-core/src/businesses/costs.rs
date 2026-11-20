//! What a building costs to run and what it is worth (§3, §5.3, §7.1; R10, R35, R41).
//!
//! `daily running cost = rent + utilities + stock` (§7.1). There are no staff costs (R35):
//! [`RunningCost`] has exactly those three lines. Rent and utilities scale with the building's
//! own volume `h × w × d`, not the plot's (R10). A building never wears, so its property value
//! is its build cost for ever (R41).

use serde::{Deserialize, Serialize};

use super::{BusinessId, Businesses};
use crate::config::Config;
use crate::economy::settlement::{rent_bill, utility_bills};
use crate::land::Map;
use crate::money::Money;
use crate::utilities::Utilities;

/// A business's daily running cost (§7.1). Three lines and nothing else: no staff (R35).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct RunningCost {
    /// Land rent by volume (§5.3), to the leaseholder or the city.
    pub rent: Money,
    /// Every utility bill by volume (§5.3), before any holder netting (R24).
    pub utilities: Money,
    /// The standing order at purchase price.
    pub stock: Money,
}

impl RunningCost {
    /// `rent + utilities + stock`.
    pub fn total(&self) -> Money {
        self.rent + self.utilities + self.stock
    }
}

impl Businesses {
    /// Property value: the build cost, unchanged by age or use (R41).
    pub fn property_value(&self, id: BusinessId) -> Option<Money> {
        self.get(id).map(|b| b.build_cost)
    }

    /// The daily running cost of business `id` in `year` (§7.1, R10, R35).
    pub fn running_cost(
        &self,
        id: BusinessId,
        config: &Config,
        map: &Map,
        year: i32,
    ) -> Option<RunningCost> {
        let b = self.get(id)?;
        let kind = config.businesses.types.get(&b.type_id)?;
        // Economy's settlement formulas, so the running cost is what 06:00 actually charges.
        let rent = rent_bill(b, map, config)?.amount;
        let utilities = utility_bills(b, kind, &Utilities::default(), config, year)
            .map(|bill| bill.amount)
            .sum();
        let stock = b
            .standing_order
            .iter()
            .map(|(good, units)| {
                config
                    .businesses
                    .goods
                    .get(good)
                    .map_or(Money::ZERO, |g| g.purchase_price.money())
                    .saturating_mul(*units)
            })
            .sum();
        Some(RunningCost {
            rent,
            utilities,
            stock,
        })
    }
}
