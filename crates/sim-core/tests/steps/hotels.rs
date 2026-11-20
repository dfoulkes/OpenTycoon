//! Steps for `features/hotels/`: the unlock and one-per-set rules through the real game
//! (`World`, shared with `steps::economy`), and tourists' nights in the city scene
//! (`steps::population::CityState`, shared with `steps::customers`).

use cucumber::{given, then, when};
use sim_core::businesses::{BuildRequest, Business, BusinessId, FitOutLevel};
use sim_core::customers::{Activity, Origin, Wave};
use sim_core::economy::{AccountId, PlayerId};
use sim_core::hotels::{self, HotelRefusal};
use sim_core::land::{BlockId, Footprint};
use sim_core::money::Money;
use sim_core::population::tourists;
use sim_core::world::Command;

use super::economy::world;
use super::population::{CityState, cohort};
use crate::SimWorld;

/// Hotel scenarios' own state.
#[derive(Debug, Default)]
pub struct HotelsState {
    /// The watched player's balance when watching began.
    watched: Option<(AccountId, Money)>,
}

fn player(n: u8) -> AccountId {
    AccountId::Player(PlayerId(n - 1))
}

fn block_named(w: &mut SimWorld, name: &str) -> BlockId {
    let i = world(w)
        .map()
        .blocks
        .iter()
        .position(|b| b.name == name)
        .unwrap_or_else(|| panic!("no block named {name}"));
    BlockId(u32::try_from(i).unwrap())
}

fn of_type_on(w: &mut SimWorld, type_id: &str, block: BlockId) -> Vec<(BusinessId, AccountId)> {
    world(w)
        .businesses()
        .by_id
        .iter()
        .filter(|(_, b)| b.type_id == type_id && b.block == block)
        .map(|(id, b)| (*id, b.owner))
        .collect()
}

// ---- the game: unlock and one per set ------------------------------------------------------

#[when(regex = r#"^player (\d+) builds a (\d+)-high (\d+)x(\d+) ([a-z_]+) on "([^"]+)"$"#)]
fn builds(
    w: &mut SimWorld,
    n: u8,
    height: u32,
    width: u32,
    depth: u32,
    kind: String,
    name: String,
) {
    let block = block_named(w, &name);
    // Along the lot's front edge, after whatever stands there (frontage, R22).
    let x = world(w)
        .businesses()
        .on_block(block)
        .map(|(_, b)| b.footprint.x + b.footprint.w)
        .max()
        .unwrap_or(0);
    let footprint = Footprint {
        x,
        y: 0,
        w: width,
        d: depth,
    };
    let game = world(w);
    game.submit(
        PlayerId(n - 1),
        Command::Build {
            block,
            type_id: kind,
            footprint,
            height,
            fit_out: FitOutLevel::Medium,
        },
    );
    game.tick();
}

#[given(regex = r#"^player (\d+) has a (\d+)-high (\d+)x(\d+) hotel on "([^"]+)"$"#)]
fn has_hotel(w: &mut SimWorld, n: u8, height: u32, width: u32, depth: u32, name: String) {
    // Built as the Build command would, without the unlock (that is unlock.feature's job).
    let block = block_named(w, &name);
    let game = world(w);
    let config = game.config().clone();
    let kind = &"hotel".to_owned();
    let request = BuildRequest {
        type_id: kind.clone(),
        block,
        footprint: Footprint {
            x: 0,
            y: 0,
            w: width,
            d: depth,
        },
        height,
        owner: player(n),
        fit_out: FitOutLevel::Medium,
    };
    let businesses = game.businesses_mut();
    let id = BusinessId(businesses.next_id);
    businesses.next_id += 1;
    businesses.by_id.insert(
        id,
        Business::new(request, &config.businesses.types[kind], Money::ZERO),
    );
}

#[then(regex = r#"^player (\d+) owns an? ([a-z_]+) on "([^"]+)"$"#)]
fn owns(w: &mut SimWorld, n: u8, kind: String, name: String) {
    let block = block_named(w, &name);
    let owners = of_type_on(w, &kind, block);
    assert!(
        owners.iter().any(|(_, o)| *o == player(n)),
        "no {kind} of player {n} on {name}: {owners:?}"
    );
}

#[then(expr = "there is no hotel on {string}")]
fn no_hotel(w: &mut SimWorld, name: String) {
    let block = block_named(w, &name);
    let config = world(w).config().clone();
    let hotels: Vec<_> = config
        .businesses
        .types
        .keys()
        .filter(|t| hotels::is_hotel(&config, t))
        .flat_map(|t| of_type_on(w, t, block))
        .collect();
    assert_eq!(hotels, Vec::new());
}

#[then(
    regex = r#"^a hotel on "([^"]+)" for player (\d+) is refused because the set (is not held in full|already has its Hotel)$"#
)]
fn refused(w: &mut SimWorld, name: String, n: u8, why: String) {
    let block = block_named(w, &name);
    let game = world(w);
    let got = hotels::check_build(
        game.config(),
        game.map(),
        game.businesses(),
        block,
        player(n),
    );
    match (why.as_str(), &got) {
        ("is not held in full", Err(HotelRefusal::NeedsFullSet { .. }))
        | ("already has its Hotel", Err(HotelRefusal::SetHasHotel { .. })) => {}
        _ => panic!("expected the set {why}, got {got:?}"),
    }
}

#[when(regex = r#"^the game runs to "(\d\d):(\d\d)" watching player (\d+)'s balance$"#)]
fn runs_watching(w: &mut SimWorld, h: u32, m: u32, n: u8) {
    let account = player(n);
    let start = world(w).ledger().balance(account);
    w.hotels.watched = Some((account, start));
    let target = h * 3600 + m * 60;
    let game = world(w);
    for _ in 0..game.clock().rules().ticks_per_day() {
        game.tick();
        if game.clock().second_of_day() == target {
            return;
        }
    }
    panic!("never reached {h}:{m}");
}

fn guests_of(w: &mut SimWorld, owner: AccountId) -> Vec<(BusinessId, u32, Money)> {
    let game = world(w);
    let config = game.config().clone();
    game.businesses()
        .by_id
        .iter()
        .filter(|(_, b)| b.owner == owner && hotels::is_hotel(&config, &b.type_id))
        .map(|(id, b)| {
            let guests = game.customers().hotel_guests.get(id).copied().unwrap_or(0);
            let price = hotels::room_price(&config, b).unwrap().0;
            (*id, guests, price)
        })
        .collect()
}

#[then(expr = "some guests have checked in at player {int}'s hotel")]
fn some_guests(w: &mut SimWorld, n: u8) {
    let total: u32 = guests_of(w, player(n)).iter().map(|g| g.1).sum();
    assert!(total > 0, "nobody checked in");
}

#[then(expr = "player {int} was paid the room price for each guest as they checked in")]
fn paid_at_check_in(w: &mut SimWorld, n: u8) {
    let (account, start) = w.hotels.watched.expect("not watching a balance");
    assert_eq!(account, player(n));
    let earned: Money = guests_of(w, account)
        .iter()
        .map(|(_, guests, price)| price.saturating_mul(i64::from(*guests)))
        .sum();
    let now = world(w).ledger().balance(account);
    assert_eq!(now - start, earned, "takings since watching began");
}

// ---- the city: tourists' nights ------------------------------------------------------------

#[given(regex = r#"^a hotel "([^"]+)" on "([^"]+)" (\d+)×(\d+)×(\d+)$"#)]
fn hotel(w: &mut SimWorld, name: String, block: String, fw: u32, fd: u32, h: u32) {
    CityState::add_building(w, &name, "hotel", &block, (fw, fd, h), AccountId::City);
}

#[given(regex = r#"^a souvenir shop "([^"]+)" on "([^"]+)" with (\d+) souvenirs$"#)]
fn souvenir_shop(w: &mut SimWorld, name: String, block: String, qty: i64) {
    let id = CityState::add_building(
        w,
        &name,
        "souvenir_shop",
        &block,
        (3, 3, 4),
        AccountId::City,
    );
    w.city
        .businesses
        .by_id
        .get_mut(&id)
        .unwrap()
        .stock
        .insert("souvenirs".into(), qty);
}

#[then(expr = "{string} has {int} rooms")]
fn has_rooms(w: &mut SimWorld, name: String, n: u32) {
    let config = CityState::config(w);
    let b = &w.city.businesses.by_id[&w.city.building(&name)];
    assert_eq!(
        hotels::rooms(b.volume(), config.businesses.hotel.room_size),
        n
    );
}

#[then(expr = "{string} holds no stock")]
fn no_stock(w: &mut SimWorld, name: String) {
    let b = &w.city.businesses.by_id[&w.city.building(&name)];
    assert_eq!(b.capacity_per_good, 0);
    assert!(b.stock.values().all(|s| *s == 0));
}

#[then(regex = r#"^"([^"]+)" is (trading|shut)$"#)]
fn trading_or_shut(w: &mut SimWorld, name: String, state: String) {
    let clock = CityState::clock(w).clone();
    let open = w.city.businesses.is_trading(w.city.building(&name), &clock);
    assert_eq!(
        open,
        state == "trading",
        "{name} at {}",
        clock.second_of_day()
    );
}

#[then(expr = "{string} is the only business trading")]
fn only_trading(w: &mut SimWorld, name: String) {
    let clock = CityState::clock(w).clone();
    let me = w.city.building(&name);
    let open: Vec<BusinessId> = w
        .city
        .businesses
        .by_id
        .keys()
        .copied()
        .filter(|id| w.city.businesses.is_trading(*id, &clock))
        .collect();
    assert_eq!(open, vec![me]);
}

#[when(expr = "the city runs through the night to {string}")]
fn through_the_night(w: &mut SimWorld, time: String) {
    super::customers::run_city_to(w, "23:59:50");
    super::customers::run_city_to(w, &time);
}

fn check_ins(w: &SimWorld) -> Vec<sim_core::customers::CheckIn> {
    w.city
        .reports
        .iter()
        .flat_map(|r| r.check_ins.iter().copied())
        .collect()
}

#[then(expr = "{string} checked in {int} guests")]
fn checked_in(w: &mut SimWorld, name: String, n: usize) {
    let id = w.city.building(&name);
    let got = check_ins(w).iter().filter(|c| c.hotel == id).count();
    assert_eq!(got, n);
}

#[then(expr = "{string} checked in more guests than {string}")]
fn more_guests(w: &mut SimWorld, a: String, b: String) {
    let (a, b) = (w.city.building(&a), w.city.building(&b));
    let all = check_ins(w);
    let count = |h| all.iter().filter(|c| c.hotel == h).count();
    assert!(count(a) > count(b), "{} vs {}", count(a), count(b));
}

#[then("every guest was a tourist")]
fn guests_are_tourists(w: &mut SimWorld) {
    let all = check_ins(w);
    assert!(!all.is_empty(), "nobody checked in");
    for c in &all {
        assert_eq!(c.origin, Origin::Tourist, "{c:?}");
        let spawned = w.city.spawned.iter().find(|s| s.id == c.customer).unwrap();
        assert_eq!(spawned.origin, Origin::Tourist);
    }
}

#[then("no commuter or resident took a room")]
fn no_commuter_rooms(w: &mut SimWorld) {
    let guests: Vec<_> = check_ins(w).iter().map(|c| c.customer).collect();
    let others = w
        .city
        .spawned
        .iter()
        .filter(|s| s.origin != Origin::Tourist)
        .collect::<Vec<_>>();
    assert!(!others.is_empty(), "no commuters or residents came out");
    assert!(others.iter().all(|s| !guests.contains(&s.id)));
}

#[then(expr = "{int} tourists are wandering the streets")]
fn wandering(w: &mut SimWorld, n: usize) {
    let wanderers = &w.city.customers.wanderers;
    assert_eq!(wanderers.len(), n);
    assert!(
        wanderers
            .iter()
            .all(|c| c.origin == Origin::Tourist && c.activity == Activity::Wandering)
    );
}

#[then("no commuter is on the map")]
fn no_commuter(w: &mut SimWorld) {
    assert!(
        w.city
            .customers
            .agents
            .iter()
            .all(|c| c.origin != Origin::Commuter)
    );
}

#[then("nobody has checked in")]
fn nobody_checked_in(w: &mut SimWorld) {
    assert_eq!(check_ins(w), Vec::new());
}

#[then("nobody is wandering")]
fn nobody_wandering(w: &mut SimWorld) {
    assert_eq!(w.city.customers.wanderers, Vec::new());
    assert!(
        w.city
            .customers
            .agents
            .iter()
            .all(|c| c.activity != Activity::Wandering)
    );
}

#[then("nobody is wandering or looking for a bed")]
fn nobody_seeking(w: &mut SimWorld) {
    assert_eq!(w.city.customers.wanderers, Vec::new());
    assert!(w.city.customers.agents.iter().all(|c| !matches!(
        c.activity,
        Activity::Wandering | Activity::SeekingBed | Activity::WalkingToHotel { .. }
    )));
}

#[then("every tourist still about is heading for a tube station")]
fn tourists_to_tube(w: &mut SimWorld) {
    let config = CityState::config(w);
    let map = CityState::map(w);
    let [lot_w, lot_d] = config.game.world.lot_size;
    let g = sim_core::customers::movement::Geometry { lot_w, lot_d };
    let year = CityState::clock(w).year();
    let stations = sim_core::customers::station_cells(&config, &map, year, g);
    for c in w
        .city
        .customers
        .agents
        .iter()
        .filter(|c| c.origin == Origin::Tourist)
    {
        assert_eq!(c.activity, Activity::Leaving, "{c:?}");
        assert!(stations.contains(&c.exit), "{c:?}");
    }
}

fn souvenir_buyers(w: &mut SimWorld, origin: Origin) -> usize {
    let config = CityState::config(w);
    let goods = sim_core::customers::shops::GoodsIndex::new(&config.businesses.goods);
    let souvenirs = goods.id("souvenirs").unwrap();
    let mut buyers: Vec<_> = w
        .city
        .reports
        .iter()
        .flat_map(|r| &r.sales)
        .filter(|s| s.good == souvenirs)
        .map(|s| s.customer)
        .filter(|id| {
            w.city
                .spawned
                .iter()
                .any(|c| c.id == *id && c.origin == origin && c.wave == Wave::Day)
        })
        .collect();
    buyers.sort();
    buyers.dedup();
    buyers.len()
}

#[then(regex = r"^between (\d+)% and (\d+)% of the day's (tourists|commuters) bought souvenirs$")]
fn souvenir_share(w: &mut SimWorld, lo: usize, hi: usize, who: String) {
    let config = CityState::config(w);
    let pool = w.city.population.visitors();
    let tourist_count = tourists(&w.city.population, &config.customers);
    let (origin, of) = if who == "tourists" {
        (Origin::Tourist, tourist_count)
    } else {
        (Origin::Commuter, pool - tourist_count)
    };
    let of = usize::try_from(of).unwrap();
    let got = souvenir_buyers(w, origin);
    assert!(
        got * 100 >= lo * of && got * 100 <= hi * of,
        "{got} of {of} {who}"
    );
}

#[then(expr = "the city has {int} visitors and {int} tourists")]
fn visitors_and_tourists(w: &mut SimWorld, visitors: u64, n: u64) {
    let config = CityState::config(w);
    assert_eq!(w.city.population.visitors(), visitors);
    assert_eq!(tourists(&w.city.population, &config.customers), n);
}

#[when(expr = "{int} of the visitors move into {string}")]
fn move_in(w: &mut SimWorld, n: u64, name: String) {
    let home = w.city.building(&name);
    let year = CityState::year(w);
    let key = cohort(w, "working", "men", "30-49");
    let pool = w.city.population.pool.get_mut(&key).unwrap();
    *pool -= n;
    for _ in 0..n {
        w.city.population.residential.house(key, home, year);
    }
}
