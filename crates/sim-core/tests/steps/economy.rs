//! Steps for `features/economy/`: the ledger, overdraft, 06:00 settlement, the midnight
//! reckoning (net worth, solvency, bankruptcy) and the sell-back, all driven through
//! `World::tick` so the tick order is under test too.
//!
//! "Player N" in the features is seat `N − 1`. Money is written as in the spec: `"-£1,234.56"`.

use std::collections::BTreeMap;

use cucumber::{given, then, when};
use sim_core::World;
use sim_core::businesses::{Business, BusinessId, FitOutLevel, build_cost};
use sim_core::economy::{AccountId, PlayerId};
use sim_core::land::{BlockId, Footprint, Lease};
use sim_core::money::{Money, Rate};
use sim_core::world::{Command, PlayerStatus};

use crate::SimWorld;

/// The game under test and the money total the last setup step left it with.
#[derive(Debug, Default)]
pub struct EconomyState {
    world: Option<World>,
    baseline: Money,
}

pub(crate) fn world(w: &mut SimWorld) -> &mut World {
    w.economy
        .world
        .as_mut()
        .expect("no game: add 'Given a new game with N players'")
}

pub(crate) fn rebaseline(w: &mut SimWorld) {
    // Net of minted allowances (R63), so the baseline only moves with created/destroyed money.
    let ledger = world(w).ledger();
    w.economy.baseline = ledger.total() - ledger.minted();
}

pub(crate) fn player(n: u8) -> PlayerId {
    assert!(n >= 1, "players are numbered from 1");
    PlayerId(n - 1)
}

pub(crate) fn account(who: &str) -> AccountId {
    if who == "the city" {
        AccountId::City
    } else {
        let n = who
            .strip_prefix("player ")
            .and_then(|n| n.parse().ok())
            .unwrap_or_else(|| panic!("not an account: {who}"));
        AccountId::Player(player(n))
    }
}

/// Parses `"£1,234.56"` or `"-£0.01"` into pence.
pub(crate) fn money(text: &str) -> Money {
    let (negative, rest) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let digits: String = rest
        .strip_prefix('£')
        .unwrap_or_else(|| panic!("money must start with £: {text}"))
        .chars()
        .filter(|c| *c != ',')
        .collect();
    let (pounds, pence) = digits.split_once('.').unwrap_or((&digits, "00"));
    assert_eq!(pence.len(), 2, "write pence as two digits: {text}");
    let total = pounds.parse::<i64>().unwrap() * 100 + pence.parse::<i64>().unwrap();
    Money::from_pence(if negative { -total } else { total })
}

pub(crate) fn parse_hm(text: &str) -> u32 {
    let (h, m) = text.split_once(':').unwrap();
    h.parse::<u32>().unwrap() * 3600 + m.parse::<u32>().unwrap() * 60
}

pub(crate) fn block_id(w: &mut SimWorld, name: &str) -> BlockId {
    let index = world(w)
        .map()
        .blocks
        .iter()
        .position(|b| b.name == name)
        .unwrap_or_else(|| panic!("no block named {name}"));
    BlockId(u32::try_from(index).unwrap())
}

pub(crate) fn business_id(w: &mut SimWorld, type_id: &str, block: &str) -> BusinessId {
    let block = block_id(w, block);
    world(w)
        .businesses()
        .by_id
        .iter()
        .find(|(_, b)| b.type_id == type_id && b.block == block)
        .map_or_else(|| panic!("no {type_id} on that block"), |(id, _)| *id)
}

pub(crate) fn business<'a>(w: &'a mut SimWorld, type_id: &str, block: &str) -> &'a mut Business {
    let id = business_id(w, type_id, block);
    world(w).businesses_mut().by_id.get_mut(&id).unwrap()
}

/// Ticks until `done` says stop (checked after each tick), at most two days.
fn run_until(w: &mut SimWorld, mut done: impl FnMut(&World, bool) -> bool) {
    let world = world(w);
    let limit = 2 * u64::from(world.clock().rules().ticks_per_day());
    for _ in 0..limit {
        let fired = world.tick();
        if done(world, fired.midnight) || world.clock().is_over() {
            return;
        }
    }
    panic!("condition not reached within two days");
}

fn run_to_time(w: &mut SimWorld, time: &str) {
    let target = parse_hm(time);
    run_until(w, |world, _| world.clock().second_of_day() == target);
}

// ---- Given ----------------------------------------------------------------------------------

#[given(expr = "a new game with {int} players")]
fn new_game(w: &mut SimWorld, players: u8) {
    // Scenarios script every player's moves; no AI seat plays (the Grocer is features/ai).
    let mut config = w.config.load_ok();
    config.ai.opponents.clear();
    w.economy.world = Some(World::new(config, 42, players));
    rebaseline(w);
}

#[given(expr = "the game has run to {string}")]
fn has_run_to(w: &mut SimWorld, time: String) {
    run_to_time(w, &time);
}

#[given(regex = r#"^(player \d+|the city) has a balance of "([^"]+)"$"#)]
fn has_balance(w: &mut SimWorld, who: String, amount: String) {
    world(w).ledger_mut().open(account(&who), money(&amount));
    rebaseline(w);
}

#[given(expr = "player {int} holds the lease on {string} bought for {string} in {int}")]
fn holds_lease(w: &mut SimWorld, n: u8, block: String, price: String, year: i32) {
    let id = block_id(w, &block);
    let term_years = world(w).config().game.leases.term_years;
    world(w).map_mut().blocks[id.0 as usize].lease = Some(Lease {
        holder: AccountId::Player(player(n)),
        start_year: year,
        term_years,
        last_price: money(&price),
    });
}

#[given(
    regex = r#"^(player \d+|the city) owns a (\d+)-high (\d+)x(\d+) "([a-z_]+)" on "([^"]+)"$"#
)]
pub(crate) fn owns_building(
    w: &mut SimWorld,
    who: String,
    height: u32,
    width: u32,
    depth: u32,
    type_id: String,
    block: String,
) {
    let block = block_id(w, &block);
    let world = world(w);
    let config = world.config().clone();
    let tier = world.map().blocks[block.0 as usize].tier;
    // Place buildings side by side along the lot's front edge (frontage, R22).
    let x = world
        .businesses()
        .by_id
        .values()
        .filter(|b| b.block == block)
        .map(|b| b.footprint.x + b.footprint.w)
        .max()
        .unwrap_or(0);
    let footprint = Footprint {
        x,
        y: 0,
        w: width,
        d: depth,
    };
    assert!(
        footprint.touches_edge(config.game.world.lot_size[0], config.game.world.lot_size[1]),
        "building does not fit on the lot's edge"
    );
    let cost = build_cost(
        height,
        footprint,
        config.businesses.build_base_rate.money(),
        config.map.tiers.get(tier).desirability_mult.rate(),
        config.businesses.fit_out.medium.build_cost_mult.rate(),
    );
    let businesses = world.businesses_mut();
    let id = BusinessId(businesses.next_id);
    businesses.next_id += 1;
    businesses.by_id.insert(
        id,
        Business {
            type_id,
            block,
            footprint,
            height,
            owner: account(&who),
            fit_out: FitOutLevel::Medium,
            build_cost: cost,
            prices: BTreeMap::default(),
            stock: BTreeMap::default(),
            tomorrow_order: BTreeMap::default(),
            open: true,
            hours: sim_core::businesses::Hours::Day,
            capacity_per_good: i64::MAX,
            standing_order: BTreeMap::default(),
            takings_today: Money::ZERO,
            sold_today: BTreeMap::default(),
            lost_sales_today: 0,
        },
    );
}

#[given(expr = "the {string} on {string} has {int} {string} in stock")]
fn has_stock(w: &mut SimWorld, type_id: String, block: String, units: i64, good: String) {
    business(w, &type_id, &block).stock.insert(good, units);
}

#[given(expr = "the {string} on {string} has ordered {int} {string} for tomorrow")]
fn has_ordered(w: &mut SimWorld, type_id: String, block: String, units: i64, good: String) {
    business(w, &type_id, &block)
        .tomorrow_order
        .insert(good, units);
}

#[given(expr = "the {string} on {string} is priced at {int}% for {string}")]
fn priced_at(w: &mut SimWorld, type_id: String, block: String, percent: i64, good: String) {
    business(w, &type_id, &block)
        .prices
        .insert(good, Rate::from_bp(percent * 100));
}

// ---- When -----------------------------------------------------------------------------------

#[when(expr = "the game runs to {string}")]
fn runs_to(w: &mut SimWorld, time: String) {
    run_to_time(w, &time);
}

#[when("the game runs to midnight")]
fn runs_to_midnight(w: &mut SimWorld) {
    run_until(w, |_, midnight| midnight);
}

#[when(expr = "the game runs for {int} days")]
fn runs_for_days(w: &mut SimWorld, days: u64) {
    let world = world(w);
    let ticks = days * u64::from(world.clock().rules().ticks_per_day());
    for _ in 0..ticks {
        world.tick();
    }
}

#[when(regex = r#"^(player \d+|the city)'s balance changes to "([^"]+)"$"#)]
fn balance_changes(w: &mut SimWorld, who: String, amount: String) {
    has_balance(w, who, amount);
}

#[when(expr = "player {int} sells the {string} on {string} to the city")]
fn sells_to_city(w: &mut SimWorld, n: u8, type_id: String, block: String) {
    let id = business_id(w, &type_id, &block);
    let world = world(w);
    world.submit(player(n), Command::SellToCity { business: id });
    world.tick();
}

// ---- Then -----------------------------------------------------------------------------------

#[then(regex = r#"^(player \d+|the city)'s balance is "([^"]+)"$"#)]
fn balance_is(w: &mut SimWorld, who: String, amount: String) {
    assert_eq!(world(w).ledger().balance(account(&who)), money(&amount));
}

#[then(expr = "player {int} has {int} strikes")]
fn strikes_are(w: &mut SimWorld, n: u8, count: u32) {
    assert_eq!(world(w).strikes(player(n)), count);
}

#[then(expr = "player {int} is still in the game")]
fn still_in(w: &mut SimWorld, n: u8) {
    assert!(
        world(w).is_active(player(n)),
        "player {n} should be in the game"
    );
}

#[then(expr = "player {int} is bankrupt")]
fn is_bankrupt(w: &mut SimWorld, n: u8) {
    let status = world(w)
        .players()
        .iter()
        .find(|(p, _)| *p == player(n))
        .map(|(_, s)| *s);
    assert_eq!(status, Some(PlayerStatus::Bankrupt));
}

#[then(expr = "the game year is {int}")]
fn game_year(w: &mut SimWorld, year: i32) {
    assert_eq!(world(w).clock().year(), year);
}

#[then(expr = "the credit limit is {string}")]
fn credit_limit(w: &mut SimWorld, amount: String) {
    assert_eq!(world(w).credit_limit(), money(&amount));
}

#[then(expr = "player {int}'s bid ceiling is {string}")]
fn bid_ceiling(w: &mut SimWorld, n: u8, amount: String) {
    assert_eq!(world(w).bid_ceiling(player(n)), Some(money(&amount)));
}

#[then(expr = "player {int} has no bid ceiling")]
fn no_bid_ceiling(w: &mut SimWorld, n: u8) {
    assert_eq!(world(w).bid_ceiling(player(n)), None);
}

#[then(regex = r#"^player (\d+) (can|cannot) spend "([^"]+)"$"#)]
fn can_spend(w: &mut SimWorld, n: u8, can: String, amount: String) {
    assert_eq!(world(w).can_afford(player(n), money(&amount)), can == "can");
}

#[then(expr = "player {int}'s net worth is {string}")]
fn net_worth_is(w: &mut SimWorld, n: u8, amount: String) {
    assert_eq!(world(w).net_worth().get(player(n)), Some(money(&amount)));
}

#[then("the game is finished")]
fn finished(w: &mut SimWorld) {
    assert!(world(w).clock().is_over());
}

#[then(expr = "the leader is player {int}")]
fn leader(w: &mut SimWorld, n: u8) {
    let ranking = world(w).net_worth().ranking();
    assert_eq!(ranking.first().map(|r| r.0), Some(player(n)), "{ranking:?}");
}

#[then(expr = "{string} is unleased")]
fn unleased(w: &mut SimWorld, block: String) {
    let id = block_id(w, &block);
    assert_eq!(world(w).map().blocks[id.0 as usize].lease, None);
}

#[then(expr = "player {int} holds the lease on {string}")]
fn holds_lease_now(w: &mut SimWorld, n: u8, block: String) {
    let id = block_id(w, &block);
    let holder = world(w).map().blocks[id.0 as usize]
        .lease
        .as_ref()
        .map(|l| l.holder);
    assert_eq!(holder, Some(AccountId::Player(player(n))));
}

#[then(regex = r#"^(player \d+|the city) owns the "([a-z_]+)" on "([^"]+)"$"#)]
fn owns_the(w: &mut SimWorld, who: String, type_id: String, block: String) {
    assert_eq!(business(w, &type_id, &block).owner, account(&who));
}

#[then(expr = "the {string} on {string} charges {int}% for {string}")]
fn charges(w: &mut SimWorld, type_id: String, block: String, percent: i64, good: String) {
    let price = business(w, &type_id, &block).prices.get(&good).copied();
    assert_eq!(price, Some(Rate::from_bp(percent * 100)));
}

#[then(expr = "the {string} on {string} has {int} {string} in stock")]
fn stock_is(w: &mut SimWorld, type_id: String, block: String, units: i64, good: String) {
    let stock = business(w, &type_id, &block).stock.get(&good).copied();
    assert_eq!(stock.unwrap_or(0), units);
}

#[then(expr = "the {string} on {string} has ordered {int} {string} for tomorrow")]
fn order_is(w: &mut SimWorld, type_id: String, block: String, units: i64, good: String) {
    let order = business(w, &type_id, &block)
        .tomorrow_order
        .get(&good)
        .copied();
    assert_eq!(order, Some(units));
}

#[then(expr = "player {int} owns nothing")]
fn owns_nothing(w: &mut SimWorld, n: u8) {
    let me = AccountId::Player(player(n));
    let world = world(w);
    assert!(world.businesses().by_id.values().all(|b| b.owner != me));
    assert!(
        world
            .map()
            .blocks
            .iter()
            .all(|b| b.lease.as_ref().is_none_or(|l| l.holder != me))
    );
}

#[then("no money was created or destroyed")]
fn conserved(w: &mut SimWorld) {
    // Only shoppers' allowances are new money (R63).
    let baseline = w.economy.baseline;
    let ledger = world(w).ledger();
    assert_eq!(ledger.total() - ledger.minted(), baseline);
}

// ---- Residents' rent (R51, R56) and the city-run shop (R4) ---------------------------------

#[then(expr = "{int} people live in the {string} on {string}")]
fn people_live_in(w: &mut SimWorld, n: u32, type_id: String, block: String) {
    let id = business_id(w, &type_id, &block);
    let occupancy = world(w).population().residential.occupancy();
    assert_eq!(occupancy.get(&id).copied().unwrap_or(0), n);
}

#[when(expr = "player {int} sets the rent at the {string} on {string} to {string}")]
fn sets_rent(w: &mut SimWorld, n: u8, type_id: String, block: String, rent: String) {
    let id = business_id(w, &type_id, &block);
    let world = world(w);
    world.submit(
        player(n),
        Command::SetRent {
            building: id,
            rent: money(&rent),
        },
    );
    world.tick();
}

/// Rent residents owe `landlord` at the next 06:00 (unchanged by settlement itself).
fn rent_owed_to(world: &World, landlord: AccountId) -> Money {
    world
        .rent_due()
        .iter()
        .filter(|d| d.landlord == landlord)
        .map(|d| d.amount)
        .sum()
}

#[then("the shoppers' purse paid every resident's rent")]
fn purse_paid_rent(w: &mut SimWorld) {
    let world = world(w);
    let rent: Money = world.rent_due().iter().map(|d| d.amount).sum();
    assert!(rent > Money::ZERO, "nobody owes rent");
    let ledger = world.ledger();
    assert_eq!(
        ledger.balance(AccountId::Shoppers),
        ledger.minted() - rent,
        "the purse should hold today's allowances less the rent"
    );
}

#[then(regex = r#"^(player \d+|the city)'s balance is "([^"]+)" plus its residents' rent$"#)]
fn balance_plus_rent(w: &mut SimWorld, who: String, amount: String) {
    let account = account(&who);
    let world = world(w);
    let rent = rent_owed_to(world, account);
    assert!(rent > Money::ZERO, "{who} is owed no rent");
    assert_eq!(world.ledger().balance(account), money(&amount) + rent);
}

#[then(regex = r#"^(player \d+|the city)'s balance is below "([^"]+)"$"#)]
fn balance_below(w: &mut SimWorld, who: String, amount: String) {
    let balance = world(w).ledger().balance(account(&who));
    assert!(balance < money(&amount), "{who} has {balance}");
}

#[then(expr = "the city has sold {string} at the {string} on {string} today")]
fn city_sold(w: &mut SimWorld, good: String, type_id: String, block: String) {
    let shop = business(w, &type_id, &block);
    assert_eq!(shop.owner, AccountId::City);
    let sold = shop.sold_today.get(&good).copied().unwrap_or(0);
    assert!(sold > 0, "no {good} sold today");
    assert!(shop.takings_today > Money::ZERO, "no takings today");
}
