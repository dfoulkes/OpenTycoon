//! The 06:00 settlement (§2.2, §5.2, §5.3, §6, §7.2, §8.2; R10, R32, R51, R56, R63).
//!
//! First, every resident's rent leaves the shoppers' purse ([`AccountId::Shoppers`], where
//! the 05:30 allowances were minted, R63) for the owner of their apartment block
//! ([`resident_rent_bills`]): rent comes out of the allowance before anyone shops (§8.2).
//! The amounts are population's ([`crate::population::residential::rent_due`]).
//!
//! Then every building, apartment blocks included (R56), pays, in this order, as [`Bill`]s:
//!
//! 1. **land rent** to the lot's leaseholder, or to the city if the lot is unleased (§5.2):
//!    `volume × land_base_rate × desirability_mult(tier)`;
//! 2. **utility bills**, one per utility, to that utility's holder (the city unless held, §6):
//!    `volume × utility_base_rate(u, year) × business_utility_factor(u)`;
//! 3. **tomorrow's stock order** to the city's wholesalers: `Σ units × purchase_price`.
//!
//! Overdraft interest, if `overdraft.interest_rate` is ever set above its default 0, is charged
//! to the city first (R32 rules there is none by default). Bills are paid with no limit check:
//! settlement may push a player past their credit limit (R32); the midnight check is the
//! punishment. A bill an account owes itself (a leaseholder's own building, a utility holder's
//! own bill, R24) nets to zero and is left out.
//!
//! The order's delivery into stock (R2) is the businesses module's job; this module only bills
//! it.

use super::{AccountId, Bill, Ledger, PlayerId};
use crate::businesses::{Business, Businesses};
use crate::config::Config;
use crate::config::businesses::BusinessType;
use crate::land::{Map, land_rent_per_day, volume};
use crate::money::{Money, Rate};
use crate::population::RentDue;
use crate::utilities::{Utilities, UtilityKind, utility_bill};

/// Utility base rate for `kind` in `year`: the telephone exchange's rate switches to the
/// internet's from the upgrade year (R11).
pub fn utility_base_rate(config: &Config, kind: UtilityKind, year: i32) -> Money {
    let rates = &config.economy.utilities.base_rates;
    match kind {
        UtilityKind::Electricity => rates.electricity.money(),
        UtilityKind::Gas => rates.gas.money(),
        UtilityKind::Water => rates.water.money(),
        UtilityKind::Telecoms if year >= config.utilities.internet.upgrade_year => {
            config.economy.utilities.internet_base_rate.money()
        }
        UtilityKind::Telecoms => rates.telecoms.money(),
    }
}

/// A business type's multiplier on one utility's base rate.
pub fn utility_factor(business_type: &BusinessType, kind: UtilityKind) -> Rate {
    let f = &business_type.utility_factor;
    match kind {
        UtilityKind::Electricity => f.electricity.rate(),
        UtilityKind::Gas => f.gas.rate(),
        UtilityKind::Water => f.water.rate(),
        UtilityKind::Telecoms => f.telecoms.rate(),
    }
}

/// One building's land rent bill (§5.3), payable to the leaseholder or the city.
pub fn rent_bill(business: &Business, map: &Map, config: &Config) -> Option<Bill> {
    let block = map.block(business.block)?;
    let amount = land_rent_per_day(
        volume(business.height, business.footprint),
        config.economy.land.base_rate.money(),
        config.map.tiers.get(block.tier).desirability_mult.rate(),
    );
    let to = block.lease.as_ref().map_or(AccountId::City, |l| l.holder);
    Some(Bill {
        from: business.owner,
        to,
        amount,
    })
}

/// One building's utility bills (§5.3, §6), one per utility, payable to each holder.
pub fn utility_bills<'a>(
    business: &'a Business,
    business_type: &'a BusinessType,
    utilities: &'a Utilities,
    config: &'a Config,
    year: i32,
) -> impl Iterator<Item = Bill> + 'a {
    let vol = volume(business.height, business.footprint);
    UtilityKind::ALL.into_iter().map(move |kind| Bill {
        from: business.owner,
        to: utilities.payee(kind),
        amount: utility_bill(
            vol,
            utility_base_rate(config, kind, year),
            utility_factor(business_type, kind),
        ),
    })
}

/// One building's stock bill for tomorrow's order: `Σ units × purchase_price`, payable to the
/// city's wholesalers. Unknown goods and non-positive quantities cost nothing.
pub fn stock_order_bill(business: &Business, config: &Config) -> Bill {
    let amount = business
        .tomorrow_order
        .iter()
        .filter_map(|(good, units)| {
            let price = config.businesses.goods.get(good)?.purchase_price.money();
            Some(price.saturating_mul((*units).max(0)))
        })
        .sum();
    Bill {
        from: business.owner,
        to: AccountId::City,
        amount,
    }
}

/// Every bill due at the 06:00 settlement in `year`, building by building in id order: rent,
/// then utilities, then stock. Self-payments and zero bills are dropped.
pub fn settlement_bills(
    map: &Map,
    businesses: &Businesses,
    utilities: &Utilities,
    config: &Config,
    year: i32,
) -> Vec<Bill> {
    let mut bills = Vec::with_capacity(businesses.by_id.len() * 6);
    for business in businesses.by_id.values() {
        bills.extend(rent_bill(business, map, config));
        if let Some(business_type) = config.businesses.types.get(&business.type_id) {
            bills.extend(utility_bills(
                business,
                business_type,
                utilities,
                config,
                year,
            ));
        }
        bills.push(stock_order_bill(business, config));
    }
    bills.retain(|b| b.from != b.to && b.amount != Money::ZERO);
    bills
}

/// Residents' rent (R51, §7.2): per apartment block, a bill from the shoppers' purse to the
/// block's owner (player or city) for the rent less its rent tax, then one to the city for the
/// tax (a study lever, 0 by default). Zero bills are dropped.
pub fn resident_rent_bills(due: &[RentDue]) -> impl Iterator<Item = Bill> + '_ {
    due.iter()
        .flat_map(|d| {
            [
                Bill {
                    from: AccountId::Shoppers,
                    to: d.landlord,
                    amount: d.amount - d.tax,
                },
                Bill {
                    from: AccountId::Shoppers,
                    to: AccountId::City,
                    amount: d.tax,
                },
            ]
        })
        .filter(|b| b.amount > Money::ZERO)
}

/// Daily overdraft interest for each overdrawn player, paid to the city. The shipped rate is
/// 0 (R32: no interest), which yields no bills.
pub fn interest_bills(
    ledger: &Ledger,
    players: impl IntoIterator<Item = PlayerId>,
    interest_rate: Rate,
) -> Vec<Bill> {
    if interest_rate == Rate::ZERO {
        return Vec::new();
    }
    players
        .into_iter()
        .filter_map(|p| {
            let account = AccountId::Player(p);
            let balance = ledger.balance(account);
            balance.is_negative().then(|| Bill {
                from: account,
                to: AccountId::City,
                amount: interest_rate.apply(-balance),
            })
        })
        .filter(|b| b.amount > Money::ZERO)
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests")]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::businesses::FitOutLevel;
    use crate::economy::test_support::shipped_config;
    use crate::land::{BlockId, Footprint, Lease};
    use proptest::prelude::*;

    const P0: AccountId = AccountId::Player(PlayerId(0));
    const P1: AccountId = AccountId::Player(PlayerId(1));

    fn bakery(owner: AccountId, block: u32, height: u32) -> Business {
        Business {
            type_id: "bakery".into(),
            block: BlockId(block),
            footprint: Footprint {
                x: 0,
                y: 0,
                w: 2,
                d: 2,
            },
            height,
            owner,
            fit_out: FitOutLevel::Medium,
            build_cost: Money::from_pounds(240),
            prices: BTreeMap::new(),
            stock: BTreeMap::new(),
            tomorrow_order: [("bread".to_owned(), 100)].into(),
            open: true,
            hours: crate::businesses::Hours::Day,
            capacity_per_good: 1_000,
            standing_order: BTreeMap::new(),
            takings_today: Money::ZERO,
            sold_today: BTreeMap::new(),
            lost_sales_today: 0,
        }
    }

    #[test]
    fn a_bakery_on_old_kent_road_pays_the_documented_bills() {
        let config = shipped_config();
        let mut map = Map::from_config(&config.map);
        map.blocks[0].lease = Some(Lease {
            holder: P1,
            start_year: 1930,
            term_years: 30,
            last_price: Money::from_pounds(2_000),
        });
        let mut businesses = Businesses::default();
        businesses
            .by_id
            .insert(crate::businesses::BusinessId(0), bakery(P0, 0, 10));
        let bills = settlement_bills(&map, &businesses, &Utilities::default(), &config, 1930);
        let to = |acct| -> Money {
            bills
                .iter()
                .filter(|b| b.to == acct)
                .map(|b| b.amount)
                .sum()
        };
        // Volume 40 on Low (×0.6): rent £1.20 to the leaseholder; utilities 40 × 6p (the
        // kitchen profile, docs/economy.md) = £2.40 and 100 bread × 36p = £36.00 to the city.
        assert_eq!(to(P1), Money::from_pence(120));
        assert_eq!(to(AccountId::City), Money::from_pence(3_840));
    }

    #[test]
    fn telecoms_switches_to_the_internet_rate_in_the_upgrade_year() {
        let config = shipped_config();
        let y = config.utilities.internet.upgrade_year;
        assert_eq!(
            utility_base_rate(&config, UtilityKind::Telecoms, y - 5),
            config.economy.utilities.base_rates.telecoms.money()
        );
        assert_eq!(
            utility_base_rate(&config, UtilityKind::Telecoms, y),
            config.economy.utilities.internet_base_rate.money()
        );
    }

    #[test]
    fn a_utility_holder_pays_itself_nothing() {
        let config = shipped_config();
        let map = Map::from_config(&config.map);
        let mut businesses = Businesses::default();
        businesses
            .by_id
            .insert(crate::businesses::BusinessId(0), bakery(P0, 0, 10));
        let mut utilities = Utilities::default();
        utilities.holders.insert(UtilityKind::Electricity, P0);
        let bills = settlement_bills(&map, &businesses, &utilities, &config, 1930);
        assert!(bills.iter().all(|b| b.from != b.to));
        let paid: Money = bills.iter().map(|b| b.amount).sum();
        // £39.60 (rent £1.20, utilities £2.40, stock £36.00) less the 80p electricity bill it
        // now pays itself.
        assert_eq!(paid, Money::from_pence(3_880));
    }

    #[test]
    fn zero_interest_charges_nothing_and_a_rate_charges_the_overdrawn_only() {
        let mut ledger = Ledger::new();
        ledger.open(P0, Money::from_pounds(-5_000));
        ledger.open(P1, Money::from_pounds(5_000));
        let players = [PlayerId(0), PlayerId(1)];
        assert_eq!(interest_bills(&ledger, players, Rate::ZERO), Vec::new());
        let bills = interest_bills(&ledger, players, Rate::from_bp(100));
        assert_eq!(
            bills,
            vec![Bill {
                from: P0,
                to: AccountId::City,
                amount: Money::from_pounds(50)
            }]
        );
    }

    proptest! {
        /// Residents' rent moves money from the shoppers' purse to landlords: the purse pays
        /// exactly what is due, the city gets each building's rent tax, each landlord the rest,
        /// and the total is unchanged.
        #[test]
        fn resident_rent_conserves_money(
            due in prop::collection::vec((0u8..3, 0u32..500, 0i64..100_000, 0i64..=10_000), 0..30),
            purse in 0i64..10_000_000,
        ) {
            let acct = |i: u8| match i { 0 => AccountId::City, 1 => P0, _ => P1 };
            let due: Vec<RentDue> = due
                .into_iter()
                .enumerate()
                .map(|(i, (who, flats, pence, tax_bp))| {
                    let amount = Money::from_pence(pence);
                    RentDue {
                        building: crate::businesses::BusinessId(u32::try_from(i).unwrap()),
                        landlord: acct(who),
                        flats_let: flats,
                        amount,
                        tax: Rate::from_bp(tax_bp).apply(amount),
                    }
                })
                .collect();
            let mut ledger = Ledger::new();
            ledger.open(AccountId::Shoppers, Money::from_pence(purse));
            let before = ledger.total();
            let bills: Vec<Bill> = resident_rent_bills(&due).collect();
            prop_assert!(bills.iter().all(|b| b.from == AccountId::Shoppers && b.amount > Money::ZERO));
            ledger.settle(&bills);
            let owed: Money = due.iter().map(|d| d.amount).sum();
            prop_assert_eq!(ledger.balance(AccountId::Shoppers), Money::from_pence(purse) - owed);
            for who in [AccountId::City, P0, P1] {
                let expected: Money = due
                    .iter()
                    .map(|d| {
                        let to_city = if who == AccountId::City { d.tax } else { Money::ZERO };
                        let to_owner = if d.landlord == who { d.amount - d.tax } else { Money::ZERO };
                        to_city + to_owner
                    })
                    .sum();
                prop_assert_eq!(ledger.balance(who), expected, "{:?}", who);
            }
            prop_assert_eq!(ledger.total(), before);
        }

        /// Money conservation (docs/engineering.md): settlement never creates or destroys money.
        #[test]
        fn settlement_conserves_money(
            buildings in prop::collection::vec((0u8..3, 0u32..9, 1u32..13, 0i64..500), 0..40),
            leases in prop::collection::vec((0usize..9, 0u8..3), 0..9),
            balances in prop::collection::vec(-2_000_000i64..2_000_000, 3),
        ) {
            let config = shipped_config();
            let acct = |i: u8| match i { 0 => AccountId::City, 1 => P0, _ => P1 };
            let mut map = Map::from_config(&config.map);
            for (block, holder) in leases {
                map.blocks[block].lease = Some(Lease {
                    holder: acct(holder),
                    start_year: 1930,
                    term_years: 30,
                    last_price: Money::from_pounds(2_000),
                });
            }
            let mut businesses = Businesses::default();
            for (i, (owner, block, height, order)) in buildings.into_iter().enumerate() {
                let mut b = bakery(acct(owner), block, height);
                b.tomorrow_order.insert("bread".into(), order);
                businesses.by_id.insert(crate::businesses::BusinessId(u32::try_from(i).unwrap()), b);
            }
            let mut ledger = Ledger::new();
            ledger.open(AccountId::City, Money::from_pence(balances[0]));
            ledger.open(P0, Money::from_pence(balances[1]));
            ledger.open(P1, Money::from_pence(balances[2]));
            let before = ledger.total();
            let mut bills = interest_bills(&ledger, [PlayerId(0), PlayerId(1)], Rate::from_bp(37));
            bills.extend(settlement_bills(&map, &businesses, &Utilities::default(), &config, 1930));
            prop_assert!(bills.iter().all(|b| b.amount >= Money::ZERO));
            ledger.settle(&bills);
            prop_assert_eq!(ledger.total(), before);
        }
    }
}
