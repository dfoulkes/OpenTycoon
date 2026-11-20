//! Steps for `features/businesses/`.
//!
//! Scenarios drive [`Businesses`] the way `World` does: the clock advances tick by tick, the
//! midnight hook ([`Businesses::midnight`]) and the 06:00 hook ([`Businesses::settlement`]) run
//! when the clock fires them, and sales go through [`Businesses::try_sell`], the API customers
//! call. Leases and the ledger belong to other modules, so they are left out here.

use std::collections::BTreeMap;

use cucumber::{given, then, when};
use sim_core::businesses::{
    BuildError, BuildRequest, BusinessId, Businesses, FitOutLevel, LostSale, PlacementError,
    WriteOff,
};
use sim_core::clock::{Clock, ClockRules};
use sim_core::config::Config;
use sim_core::economy::{AccountId, Bill, PlayerId, settlement};
use sim_core::land::{BlockId, Footprint, Map};
use sim_core::money::{Money, Rate};
use sim_core::utilities::Utilities;

use crate::SimWorld;

/// The businesses under test and what the last hooks produced.
#[derive(Debug, Default)]
pub struct BusinessesState {
    sim: Option<Sim>,
}

#[derive(Debug)]
struct Sim {
    config: Config,
    map: Map,
    clock: Clock,
    businesses: Businesses,
    utilities: Utilities,
    /// The last business built of each type.
    by_type: BTreeMap<String, BusinessId>,
    last_build: Option<Result<(BusinessId, Money), BuildError>>,
    write_offs: Vec<WriteOff>,
    stock_bills: Vec<Bill>,
    utility_bills: Vec<Bill>,
    /// Units delivered at the last 06:00.
    delivered: i64,
}

fn sim(w: &mut SimWorld) -> &mut Sim {
    if w.businesses.sim.is_none() {
        let config = w.config.load_ok();
        w.businesses.sim = Some(Sim {
            map: Map::from_config(&config.map),
            clock: Clock::new(ClockRules::from_config(&config.game.clock)),
            businesses: Businesses::default(),
            utilities: Utilities::default(),
            by_type: BTreeMap::new(),
            last_build: None,
            write_offs: Vec::new(),
            stock_bills: Vec::new(),
            utility_bills: Vec::new(),
            delivered: 0,
            config,
        });
    }
    w.businesses.sim.as_mut().unwrap()
}

impl Sim {
    fn id(&self, type_id: &str) -> BusinessId {
        *self
            .by_type
            .get(type_id)
            .unwrap_or_else(|| panic!("no {type_id} built"))
    }

    /// The business of this type, checking `player` owns it (levers are the owner's).
    fn owned(&self, type_id: &str, player_no: u8) -> BusinessId {
        let id = self.id(type_id);
        assert_eq!(self.businesses.get(id).unwrap().owner, player(player_no));
        id
    }

    fn block(&self, name: &str) -> BlockId {
        let i = self
            .map
            .blocks
            .iter()
            .position(|b| b.name == name)
            .unwrap_or_else(|| panic!("no block {name}"));
        BlockId(u32::try_from(i).unwrap())
    }

    /// One tick, running the businesses' midnight and 06:00 hooks as `World::tick` does.
    fn tick(&mut self) {
        let fired = self.clock.advance();
        if fired.midnight {
            self.write_offs = self.businesses.midnight();
        }
        if fired.settlement {
            // Economy's 06:00 bills first (as World::settlement), then delivery (R2).
            let year = self.clock.year();
            self.stock_bills.clear();
            self.utility_bills.clear();
            for b in self.businesses.by_id.values() {
                let stock = settlement::stock_order_bill(b, &self.config);
                if stock.amount != Money::ZERO {
                    self.stock_bills.push(stock);
                }
                let kind = &self.config.businesses.types[&b.type_id];
                self.utility_bills.extend(
                    settlement::utility_bills(b, kind, &self.utilities, &self.config, year)
                        .filter(|bill| bill.from != bill.to && bill.amount != Money::ZERO),
                );
            }
            self.delivered = self
                .businesses
                .deliver_orders()
                .iter()
                .map(|d| d.units)
                .sum();
        }
    }

    fn run_to(&mut self, second_of_day: u32) {
        let step = self.clock.rules().tick_game_seconds;
        assert_eq!(second_of_day % step, 0, "not on a tick");
        loop {
            self.tick();
            if self.clock.second_of_day() == second_of_day {
                break;
            }
        }
    }

    fn build(&mut self, request: BuildRequest) {
        let type_id = request.type_id.clone();
        let year = self.clock.year();
        let result = self
            .businesses
            .build(request, &self.config, &self.map, year);
        if let Ok((id, _)) = &result {
            self.by_type.insert(type_id, *id);
        }
        self.last_build = Some(result);
    }

    fn sell(&mut self, type_id: &str, good: &str, qty: i64) -> Result<Money, LostSale> {
        let id = self.id(type_id);
        self.businesses
            .try_sell(id, good, qty, &self.clock, &self.config.businesses)
    }
}

fn player(n: u8) -> AccountId {
    AccountId::Player(PlayerId(n))
}

fn parse_hms(text: &str) -> u32 {
    let parts: Vec<u32> = text.split(':').map(|p| p.parse().unwrap()).collect();
    parts[0] * 3600 + parts[1] * 60 + parts.get(2).copied().unwrap_or(0)
}

#[allow(clippy::too_many_arguments, reason = "mirrors the step's captures")]
fn request(
    type_id: &str,
    owner: u8,
    block: BlockId,
    w: u32,
    d: u32,
    at: Option<(u32, u32)>,
    height: u32,
    fit_out: Option<&str>,
) -> BuildRequest {
    let (x, y) = at.unwrap_or((0, 0));
    BuildRequest {
        type_id: type_id.to_owned(),
        block,
        footprint: Footprint { x, y, w, d },
        height,
        owner: player(owner),
        fit_out: match fit_out {
            Some("low") => FitOutLevel::Low,
            Some("high") => FitOutLevel::High,
            _ => FitOutLevel::Medium,
        },
    }
}

#[allow(clippy::too_many_arguments, reason = "mirrors the step's captures")]
fn build_step(
    w: &mut SimWorld,
    owner: u8,
    type_id: &str,
    size: (u32, u32),
    at: Option<(u32, u32)>,
    height: u32,
    block: &str,
    fit_out: Option<&str>,
) {
    let s = sim(w);
    let block = s.block(block);
    let req = request(type_id, owner, block, size.0, size.1, at, height, fit_out);
    s.build(req);
}

#[given(
    regex = r"^player (\d+) owns an? (\w+) (\d+)×(\d+) and (\d+) high on ([A-Z][\w']*(?: [A-Z][\w']*)*)$"
)]
fn owns(w: &mut SimWorld, owner: u8, t: String, bw: u32, bd: u32, h: u32, block: String) {
    build_step(w, owner, &t, (bw, bd), None, h, &block, None);
    let got = &sim(w).last_build;
    assert!(matches!(got, Some(Ok(_))), "setup build failed: {got:?}");
}

#[when(
    regex = r"^player (\d+) builds an? (\w+) (\d+)×(\d+) and (\d+) high on ([A-Z][\w']*(?: [A-Z][\w']*)*)$"
)]
fn builds(w: &mut SimWorld, owner: u8, t: String, bw: u32, bd: u32, h: u32, block: String) {
    build_step(w, owner, &t, (bw, bd), None, h, &block, None);
}

#[when(
    regex = r"^player (\d+) builds an? (\w+) (\d+)×(\d+) at (\d+),(\d+) and (\d+) high on ([A-Z][\w']*(?: [A-Z][\w']*)*)$"
)]
#[allow(clippy::too_many_arguments, reason = "one argument per capture")]
fn builds_at(
    w: &mut SimWorld,
    owner: u8,
    t: String,
    bw: u32,
    bd: u32,
    col: u32,
    row: u32,
    h: u32,
    block: String,
) {
    build_step(w, owner, &t, (bw, bd), Some((col, row)), h, &block, None);
}

#[when(
    regex = r"^player (\d+) builds an? (\w+) (\d+)×(\d+) and (\d+) high on ([A-Z][\w']*(?: [A-Z][\w']*)*) with an? (low|medium|high) fit-out$"
)]
#[allow(clippy::too_many_arguments, reason = "one argument per capture")]
fn builds_fitted(
    w: &mut SimWorld,
    owner: u8,
    t: String,
    bw: u32,
    bd: u32,
    h: u32,
    block: String,
    fit: String,
) {
    build_step(w, owner, &t, (bw, bd), None, h, &block, Some(&fit));
}

#[given(regex = r"^it is (\d{4})$")]
fn it_is_year(w: &mut SimWorld, year: i32) {
    let s = sim(w);
    while s.clock.year() < year {
        s.run_to(0);
    }
    assert_eq!(s.clock.year(), year);
}

#[given(regex = r"^the (\w+) has (\d+) (\w+) in stock$")]
fn has_stock(w: &mut SimWorld, type_id: String, units: i64, good: String) {
    let s = sim(w);
    let id = s.id(&type_id);
    let b = s.businesses.by_id.get_mut(&id).unwrap();
    assert!(b.sells(&good), "{type_id} doesn't sell {good}");
    b.stock.insert(good, units);
}

#[then(regex = r"^the (\w+) has (\d+) (\w+) in stock$")]
fn stock_is(w: &mut SimWorld, type_id: String, units: i64, good: String) {
    let s = sim(w);
    let id = s.id(&type_id);
    assert_eq!(s.businesses.get(id).unwrap().stock_of(&good), units);
}

#[when(expr = "the day runs to {string}")]
fn day_runs_to(w: &mut SimWorld, time: String) {
    sim(w).run_to(parse_hms(&time));
}

#[when(regex = r"^the clock runs on (\d+) days?$")]
fn runs_on(w: &mut SimWorld, days: u32) {
    let s = sim(w);
    let now = s.clock.second_of_day();
    for _ in 0..days {
        s.run_to(now);
    }
}

#[then(regex = r#"^selling (\d+) (\w+) at the (\w+) earns "(.+)"$"#)]
fn sell_earns(w: &mut SimWorld, qty: i64, good: String, type_id: String, amount: String) {
    let got = sim(w).sell(&type_id, &good, qty);
    assert_eq!(got.map(|m| m.to_string()), Ok(amount));
}

#[then(
    regex = r"^selling (\d+) (\w+) at the (\w+) is lost because it (is closed|is out of stock|does not sell it)$"
)]
fn sell_lost(w: &mut SimWorld, qty: i64, good: String, type_id: String, why: String) {
    let got = sim(w).sell(&type_id, &good, qty);
    let ok = match (why.as_str(), got) {
        ("is closed", Err(LostSale::Closed))
        | ("is out of stock", Err(LostSale::OutOfStock { .. }))
        | ("does not sell it", Err(LostSale::NotSold)) => true,
        (_, other) => panic!("expected lost sale ({why}), got {other:?}"),
    };
    assert!(ok);
}

#[when(regex = r"^(\d+) (\w+) are sold at the (\w+)$")]
fn are_sold(w: &mut SimWorld, qty: i64, good: String, type_id: String) {
    let got = sim(w).sell(&type_id, &good, qty);
    assert!(got.is_ok(), "sale failed: {got:?}");
}

#[when(regex = r"^player (\d+) (closes|opens) the (\w+)$")]
fn toggle(w: &mut SimWorld, owner: u8, what: String, type_id: String) {
    let s = sim(w);
    let id = s.owned(&type_id, owner);
    s.businesses.set_open(id, what == "opens").unwrap();
}

#[given(regex = r"^player (\d+) prices (\w+) at the (\w+) at (\d+)% of the reference price$")]
fn prices(w: &mut SimWorld, owner: u8, good: String, type_id: String, percent: i64) {
    let s = sim(w);
    let id = s.owned(&type_id, owner);
    s.businesses
        .set_price(id, &good, Rate::from_bp(percent * 100))
        .unwrap();
}

#[given(regex = r"^player (\d+) orders (\d+) (\w+) for tomorrow at the (\w+)$")]
fn given_orders(w: &mut SimWorld, owner: u8, qty: i64, good: String, type_id: String) {
    orders(w, owner, qty, good, type_id);
}

#[when(regex = r"^player (\d+) orders (\d+) (\w+) for tomorrow at the (\w+)$")]
fn orders(w: &mut SimWorld, owner: u8, qty: i64, good: String, type_id: String) {
    let s = sim(w);
    let id = s.owned(&type_id, owner);
    s.businesses.set_order(id, &good, qty).unwrap();
}

#[then(regex = r"^tomorrow's order at the (\w+) is (\d+) (\w+)$")]
fn order_is(w: &mut SimWorld, type_id: String, qty: i64, good: String) {
    let s = sim(w);
    let id = s.id(&type_id);
    let b = s.businesses.get(id).unwrap();
    assert_eq!(b.standing_order.get(&good).copied().unwrap_or(0), qty);
}

#[then(regex = r#"^the (\w+) has taken "(.+)" today$"#)]
fn taken(w: &mut SimWorld, type_id: String, amount: String) {
    let s = sim(w);
    let id = s.id(&type_id);
    assert_eq!(
        s.businesses.get(id).unwrap().takings_today.to_string(),
        amount
    );
}

#[then(regex = r"^the (\w+) has lost (\d+) sales? today$")]
fn lost_today(w: &mut SimWorld, type_id: String, n: u32) {
    let s = sim(w);
    let id = s.id(&type_id);
    assert_eq!(s.businesses.get(id).unwrap().lost_sales_today, n);
}

#[then(regex = r#"^(\d+) (\w+) worth "(.+)" were written off at the (\w+)$"#)]
fn written_off(w: &mut SimWorld, units: i64, good: String, value: String, type_id: String) {
    let s = sim(w);
    let id = s.id(&type_id);
    let w_off: Vec<&WriteOff> = s
        .write_offs
        .iter()
        .filter(|o| o.business == id && o.good == good)
        .collect();
    assert_eq!(w_off.len(), 1, "write-offs: {:?}", s.write_offs);
    assert_eq!(w_off[0].units, units);
    assert_eq!(w_off[0].value(&s.config.businesses).to_string(), value);
}

fn charged(bills: &[Bill], payer: AccountId) -> Money {
    bills
        .iter()
        .filter(|b| b.from == payer)
        .map(|b| b.amount)
        .sum()
}

#[then(regex = r#"^the 06:00 bills charge player (\d+) "(.+)" for stock$"#)]
fn stock_bill(w: &mut SimWorld, owner: u8, amount: String) {
    let s = sim(w);
    assert_eq!(charged(&s.stock_bills, player(owner)).to_string(), amount);
}

#[then(regex = r#"^the 06:00 bills charge player (\d+) "(.+)" for utilities, paid to the city$"#)]
fn utility_bill_paid(w: &mut SimWorld, owner: u8, amount: String) {
    let s = sim(w);
    assert_ne!(s.utility_bills.len(), 0, "no utility bills");
    assert!(s.utility_bills.iter().all(|b| b.to == AccountId::City));
    assert_eq!(charged(&s.utility_bills, player(owner)).to_string(), amount);
}

#[then("no bills have been raised")]
fn no_bills(w: &mut SimWorld) {
    let s = sim(w);
    assert!(s.stock_bills.is_empty() && s.utility_bills.is_empty());
}

#[then(
    regex = r"^the building (goes up|is refused as too tall|is refused as too short|is refused as too small|is refused as outside the lot|is refused for having no street frontage|is refused for overlapping another building)$"
)]
fn building_outcome(w: &mut SimWorld, outcome: String) {
    let got = sim(w).last_build.clone().expect("nothing was built");
    let ok = match (outcome.as_str(), &got) {
        ("goes up", Ok(_))
        | ("is refused as too tall", Err(BuildError::TooTall { .. }))
        | ("is refused as too short", Err(BuildError::TooShort))
        | ("is refused as too small", Err(BuildError::FootprintTooSmall { .. }))
        | (
            "is refused as outside the lot",
            Err(BuildError::Placement(PlacementError::OutsideLot)),
        )
        | (
            "is refused for having no street frontage",
            Err(BuildError::Placement(PlacementError::NoStreetFrontage)),
        )
        | (
            "is refused for overlapping another building",
            Err(BuildError::Placement(PlacementError::Overlaps(_))),
        ) => true,
        _ => panic!("expected the building {outcome}, got {got:?}"),
    };
    assert!(ok);
}

#[then(regex = r#"^the building cost "(.+)"$"#)]
fn building_cost(w: &mut SimWorld, amount: String) {
    let got = sim(w).last_build.clone().expect("nothing was built");
    assert_eq!(got.map(|(_, cost)| cost.to_string()), Ok(amount));
}

#[then(regex = r"^the (\w+) stocks only (\w+), up to (\d+) units$")]
fn stocks_only(w: &mut SimWorld, type_id: String, good: String, units: i64) {
    let s = sim(w);
    let b = s.businesses.get(s.id(&type_id)).unwrap();
    assert_eq!(b.prices.keys().collect::<Vec<_>>(), vec![&good]);
    assert_eq!(b.capacity_per_good, units);
}

#[then(regex = r#"^the (\w+)'s property value is "(.+)"$"#)]
fn property_value(w: &mut SimWorld, type_id: String, amount: String) {
    let s = sim(w);
    let id = s.id(&type_id);
    assert_eq!(
        s.businesses.property_value(id).map(|m| m.to_string()),
        Some(amount)
    );
}

#[then(regex = r"^the (\w+)'s volume is (\d+)$")]
fn volume_is(w: &mut SimWorld, type_id: String, v: i64) {
    let s = sim(w);
    assert_eq!(s.businesses.get(s.id(&type_id)).unwrap().volume(), v);
}

#[then(regex = r"^the (\w+) can hold (\d+) (\w+)$")]
fn can_hold(w: &mut SimWorld, type_id: String, units: i64, good: String) {
    let s = sim(w);
    let b = s.businesses.get(s.id(&type_id)).unwrap();
    assert!(b.sells(&good));
    assert_eq!(b.capacity_per_good, units);
}

fn running(w: &mut SimWorld, type_id: &str) -> sim_core::businesses::RunningCost {
    let s = sim(w);
    let id = s.id(type_id);
    let year = s.clock.year();
    s.businesses
        .running_cost(id, &s.config, &s.map, year)
        .unwrap()
}

#[then(regex = r#"^the (\w+)'s daily rent is "(.+)"$"#)]
fn rent_is(w: &mut SimWorld, type_id: String, amount: String) {
    assert_eq!(running(w, &type_id).rent.to_string(), amount);
}

#[then(regex = r#"^the (\w+)'s daily utility bill is "(.+)"$"#)]
fn utilities_is(w: &mut SimWorld, type_id: String, amount: String) {
    assert_eq!(running(w, &type_id).utilities.to_string(), amount);
}

#[then(regex = r#"^the (\w+)'s daily running cost is "(.+)"$"#)]
fn running_is(w: &mut SimWorld, type_id: String, amount: String) {
    assert_eq!(running(w, &type_id).total().to_string(), amount);
}

#[then(
    regex = r#"^it is made of rent "(.+)", utilities "(.+)" and stock "(.+)" and nothing else$"#
)]
fn made_of(w: &mut SimWorld, rent: String, utilities: String, stock: String) {
    let types: Vec<String> = sim(w).by_type.keys().cloned().collect();
    assert_eq!(types.len(), 1, "one business expected");
    let cost = running(w, &types[0]);
    // Destructure every field: a new cost line (staff, R35) would fail to compile here.
    let sim_core::businesses::RunningCost {
        rent: r,
        utilities: u,
        stock: s,
    } = cost;
    assert_eq!(
        (r.to_string(), u.to_string(), s.to_string()),
        (rent, utilities, stock)
    );
    assert_eq!(cost.total(), r + u + s);
}

#[then(regex = r"^the lot is (\d+)×(\d+)$")]
fn lot_is(w: &mut SimWorld, lw: u32, ld: u32) {
    assert_eq!(sim(w).config.game.world.lot_size, [lw, ld]);
}

#[then(regex = r#"^(\d+) (\w+) were delivered and player (\d+) was billed "(.+)" for them$"#)]
fn delivered_and_billed(w: &mut SimWorld, units: i64, good: String, owner: u8, amount: String) {
    let s = sim(w);
    let price = s.config.businesses.goods[&good].purchase_price.money();
    assert_eq!(s.delivered, units, "delivered");
    let billed = charged(&s.stock_bills, player(owner));
    assert_eq!(billed.to_string(), amount, "billed");
    assert_eq!(
        billed,
        price.saturating_mul(s.delivered),
        "billed = delivered × price"
    );
}
