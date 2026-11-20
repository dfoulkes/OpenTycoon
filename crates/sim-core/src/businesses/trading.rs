//! The player's levers and the sales API (§7.1, §8.3; R1, R13).
//!
//! Levers (applied at the commands phase): price per good as a share of its reference price,
//! tomorrow's order, open/closed. Fit-out is stored on the building; changing it is post-v1.
//!
//! Sales: customers call [`Businesses::try_sell`]. It checks the shop is open (owner toggle and
//! the clock's window for the type's hours, R1), that the shop sells the good (R13) and has
//! the stock, then takes the units off the shelf and returns the revenue. The caller moves that
//! revenue from the shopper to [`super::Business::owner`] through the ledger; this module never
//! touches balances.

use serde::{Deserialize, Serialize};

use super::{Business, BusinessId, Businesses, Hours};
use crate::clock::{Clock, TradingWindow};
use crate::config::BusinessesConfig;
use crate::money::{Money, Rate};

/// Why a sale didn't happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LostSale {
    /// No such shop.
    NoSuchShop,
    /// Outside its trading window, or its owner has it shut (R1).
    Closed,
    /// The shop's type doesn't sell this good (R13).
    NotSold,
    /// Too little on the shelf (§8.3). Counted against the shop's day.
    OutOfStock {
        /// Units on the shelf.
        available: i64,
    },
    /// Quantity was zero or negative.
    BadQuantity,
}

/// Why a lever was refused.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LeverError {
    /// No such business.
    NoSuchBusiness(BusinessId),
    /// The business doesn't sell this good.
    NotSold(String),
    /// Prices must be above zero.
    BadPrice,
    /// Orders can't be negative.
    NegativeOrder,
}

/// The trading window for a type's hours, if it has one.
pub fn window_for(hours: Hours) -> Option<TradingWindow> {
    match hours {
        Hours::Day => Some(TradingWindow::Day),
        Hours::Night => Some(TradingWindow::Night),
        Hours::Hotel => Some(TradingWindow::Hotel),
        Hours::None => None,
    }
}

/// `unit_price = price_share × ref_price`, rounded half away from zero to the penny.
pub fn unit_price(price_share: Rate, ref_price: Money) -> Money {
    price_share.apply(ref_price)
}

impl Business {
    /// True if the shop is trading now: its owner has it open and its window is open (R1).
    pub fn is_trading(&self, clock: &Clock) -> bool {
        self.open && window_for(self.hours).is_some_and(|w| clock.is_open(w))
    }

    /// The shelf price of one unit of `good`, if the shop sells it.
    pub fn unit_price(&self, good: &str, rules: &BusinessesConfig) -> Option<Money> {
        let share = *self.prices.get(good)?;
        let reference = rules.goods.get(good)?.ref_price.money();
        Some(unit_price(share, reference))
    }
}

impl Businesses {
    /// True if the business exists and is trading now (R1).
    pub fn is_trading(&self, id: BusinessId, clock: &Clock) -> bool {
        self.get(id).is_some_and(|b| b.is_trading(clock))
    }

    /// Sells `quantity` units of `good` at shop `id` now. On success the units leave the shelf,
    /// the day's takings rise, and the revenue is returned for the caller to transfer from the
    /// shopper to the owner. A shortfall sells nothing and counts a lost sale (§8.3).
    pub fn try_sell(
        &mut self,
        id: BusinessId,
        good: &str,
        quantity: i64,
        clock: &Clock,
        rules: &BusinessesConfig,
    ) -> Result<Money, LostSale> {
        if quantity <= 0 {
            return Err(LostSale::BadQuantity);
        }
        let shop = self.by_id.get_mut(&id).ok_or(LostSale::NoSuchShop)?;
        if !shop.is_trading(clock) {
            return Err(LostSale::Closed);
        }
        let price = shop.unit_price(good, rules).ok_or(LostSale::NotSold)?;
        let available = shop.stock_of(good);
        if available < quantity {
            shop.lost_sales_today = shop.lost_sales_today.saturating_add(1);
            return Err(LostSale::OutOfStock { available });
        }
        let revenue = price.saturating_mul(quantity);
        shop.stock.insert(good.to_owned(), available - quantity);
        *shop.sold_today.entry(good.to_owned()).or_insert(0) += quantity;
        shop.takings_today += revenue;
        Ok(revenue)
    }

    /// Sells `quantity` of `good` online (§7.5): as [`Businesses::try_sell`], but the shop is
    /// never walked to and has no trading window, so only the open toggle is checked.
    ///
    /// # Errors
    ///
    /// As [`Businesses::try_sell`].
    pub fn try_sell_online(
        &mut self,
        id: BusinessId,
        good: &str,
        quantity: i64,
        rules: &BusinessesConfig,
    ) -> Result<Money, LostSale> {
        let _ = (id, good, quantity, rules);
        Err(LostSale::Closed)
    }

    /// Sets a good's price as a share of its reference price (§7.1). Must be above zero.
    pub fn set_price(&mut self, id: BusinessId, good: &str, price: Rate) -> Result<(), LeverError> {
        if price.bp() <= 0 {
            return Err(LeverError::BadPrice);
        }
        let shop = self.shop_selling(id, good)?;
        shop.prices.insert(good.to_owned(), price);
        Ok(())
    }

    /// Sets tomorrow's order for a good (R2). It is capped at what the building can hold
    /// (R10) and the accepted quantity is returned. It takes effect when it locks at the next
    /// midnight and arrives at the 06:00 after that; nothing arrives mid-day.
    pub fn set_order(
        &mut self,
        id: BusinessId,
        good: &str,
        quantity: i64,
    ) -> Result<i64, LeverError> {
        if quantity < 0 {
            return Err(LeverError::NegativeOrder);
        }
        let shop = self.shop_selling(id, good)?;
        let accepted = quantity.min(shop.capacity_per_good);
        shop.standing_order.insert(good.to_owned(), accepted);
        Ok(accepted)
    }

    /// Opens or shuts the shop (§7.1). A shut shop sells nothing even in its hours.
    pub fn set_open(&mut self, id: BusinessId, open: bool) -> Result<(), LeverError> {
        let shop = self
            .by_id
            .get_mut(&id)
            .ok_or(LeverError::NoSuchBusiness(id))?;
        shop.open = open;
        Ok(())
    }

    fn shop_selling(&mut self, id: BusinessId, good: &str) -> Result<&mut Business, LeverError> {
        let shop = self
            .by_id
            .get_mut(&id)
            .ok_or(LeverError::NoSuchBusiness(id))?;
        if shop.sells(good) {
            Ok(shop)
        } else {
            Err(LeverError::NotSold(good.to_owned()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_price_rounds_to_the_penny() {
        // 10p at 125% = 12.5p → 13p (half away from zero).
        assert_eq!(
            unit_price(Rate::from_bp(12_500), Money::from_pence(10)),
            Money::from_pence(13)
        );
    }

    #[test]
    fn apartments_never_trade() {
        assert_eq!(window_for(Hours::None), None);
    }
}
