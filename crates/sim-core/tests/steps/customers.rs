//! Steps for `features/customers/`. The city scene lives in [`super::population::CityState`].
#![allow(
    clippy::many_single_char_names,
    reason = "step captures read as class, sex, band: c, s, b"
)]

use std::collections::BTreeSet;

use cucumber::{given, then, when};
use sim_core::businesses::Hours;
use sim_core::customers::movement::Geometry;
use sim_core::customers::shops::{GoodsIndex, shop_offers};
use sim_core::customers::{
    Activity, Origin, PhaseInputs, Wave, choose_shop, front_doors, run_phase, scoring, shopping,
    station_cells,
};
use sim_core::economy::AccountId;
use sim_core::money::Rate;
use sim_core::population::allowance;

use super::population::{CityState, class, cohort, pounds};
use crate::SimWorld;

fn geometry(w: &mut SimWorld) -> Geometry {
    let config = CityState::config(w);
    let [lot_w, lot_d] = config.game.world.lot_size;
    Geometry { lot_w, lot_d }
}

/// Runs the customers phase for one tick.
fn tick(w: &mut SimWorld) {
    let config = CityState::config(w);
    let map = CityState::map(w);
    CityState::clock(w).advance();
    let clock = w.city.clock.clone().unwrap();
    let mut rng = w.city.rng().clone();
    let city = &mut w.city;
    let report = run_phase(
        &mut city.customers,
        PhaseInputs {
            config: &config,
            map: &map,
            clock: &clock,
            population: &city.population,
            modifiers: &sim_core::events::Modifiers::default(),
        },
        &mut city.businesses,
        &mut rng,
    );
    city.rng = Some(rng);
    for c in &city.customers.agents {
        if !city.spawned.iter().any(|s| s.id == c.id) {
            city.spawned.push(c.clone());
        }
    }
    city.reports.push(report);
}

fn parse_hms(text: &str) -> u32 {
    let parts: Vec<u32> = text.split(':').map(|p| p.parse().unwrap()).collect();
    parts[0] * 3600 + parts[1] * 60 + parts.get(2).copied().unwrap_or(0)
}

#[given(
    regex = r#"^an? (bakery|grocery|clothes_store|bar|restaurant) "([^"]+)" on "([^"]+)" with (\d+) (\w+)$"#
)]
fn shop(w: &mut SimWorld, kind: String, name: String, block: String, qty: i64, good: String) {
    let id = CityState::add_building(w, &name, &kind, &block, (3, 3, 4), AccountId::City);
    w.city
        .businesses
        .by_id
        .get_mut(&id)
        .unwrap()
        .stock
        .insert(good, qty);
}

#[given(expr = "{string} prices {word} at {int}% of the reference price")]
fn prices(w: &mut SimWorld, name: String, good: String, pct: i64) {
    let id = w.city.building(&name);
    w.city
        .businesses
        .by_id
        .get_mut(&id)
        .unwrap()
        .prices
        .insert(good, Rate::from_bp(pct * 100));
}

#[given(regex = r#"^"([^"]+)" has a reputation of ([\d.]+)$"#)]
fn set_reputation(w: &mut SimWorld, name: String, rep: f64) {
    let id = w.city.building(&name);
    w.city
        .customers
        .reputation
        .insert(id, scoring::unit_to_rate(rep));
}

#[then(regex = r#"^"([^"]+)" has a reputation of ([\d.]+)$"#)]
fn reputation_is(w: &mut SimWorld, name: String, rep: f64) {
    let config = CityState::config(w);
    let id = w.city.building(&name);
    assert_eq!(
        w.city.customers.reputation(id, &config.customers),
        scoring::unit_to_rate(rep)
    );
}

#[when(regex = r#"^"([^"]+)" sells out of (\w+)$"#)]
fn sells_out(w: &mut SimWorld, name: String, good: String) {
    let id = w.city.building(&name);
    w.city
        .businesses
        .by_id
        .get_mut(&id)
        .unwrap()
        .stock
        .insert(good, 0);
}

#[when(regex = r#"^"([^"]+)" gets (\d+) (\w+)$"#)]
fn gets(w: &mut SimWorld, name: String, qty: i64, good: String) {
    let id = w.city.building(&name);
    w.city
        .businesses
        .by_id
        .get_mut(&id)
        .unwrap()
        .stock
        .insert(good, qty);
}

#[then(
    regex = r#"^an? (working|middle|upper) (man|woman) aged "([^"]+)" has a daily allowance of £([\d.]+)$"#
)]
fn daily_allowance(w: &mut SimWorld, c: String, s: String, b: String, amount: String) {
    let config = CityState::config(w);
    let key = cohort(w, &c, &s, &b);
    assert_eq!(allowance(&config.customers, key), pounds(&amount));
}

#[then(
    regex = r#"^P\((\w+) \| (working|middle|upper) (man|woman) aged "([^"]+)", (day|night), (\d+)\) is ([\d.]+)$"#
)]
#[allow(
    clippy::too_many_arguments,
    reason = "one capture per term of P(good | ...)"
)]
fn desire(
    w: &mut SimWorld,
    good: String,
    c: String,
    s: String,
    b: String,
    window: String,
    year: i32,
    p: f64,
) {
    let config = CityState::config(w);
    let key = cohort(w, &c, &s, &b);
    let hours = if window == "day" {
        Hours::Day
    } else {
        Hours::Night
    };
    let weights = shopping::desire_weights(&config.customers, key, hours, year);
    let got = weights.get(good.as_str()).copied().unwrap_or(0.0);
    assert!((got - p).abs() < 1e-9, "P({good}) = {got}, expected {p}");
}

#[then(
    regex = r#"^an? (working|middle|upper) (man|woman) aged "([^"]+)" shopping by (day|night) wants "([^"]+)"$"#
)]
fn wants(w: &mut SimWorld, c: String, s: String, b: String, window: String, list: String) {
    let config = CityState::config(w);
    let key = cohort(w, &c, &s, &b);
    let hours = if window == "day" {
        Hours::Day
    } else {
        Hours::Night
    };
    let goods = GoodsIndex::new(&config.businesses.goods);
    for _ in 0..20 {
        let rolled = shopping::roll_list(&config.customers, &goods, key, hours, 1930, w.city.rng());
        let names: Vec<&str> = rolled.iter().map(|g| goods.name(*g)).collect();
        assert_eq!(names.join(", "), list);
    }
}

#[when(expr = "the city runs to {string}")]
fn city_runs_to(w: &mut SimWorld, time: String) {
    let target = parse_hms(&time);
    loop {
        tick(w);
        if CityState::clock(w).second_of_day() == target {
            break;
        }
    }
}

#[when(expr = "the city runs for {int} tick(s)")]
fn city_runs_ticks(w: &mut SimWorld, n: u32) {
    for _ in 0..n {
        tick(w);
    }
}

fn walking_to(w: &SimWorld, shop: sim_core::businesses::BusinessId) -> bool {
    w.city
        .customers
        .agents
        .iter()
        .any(|c| matches!(c.activity, Activity::Walking { shop: s, .. } if s == shop))
}

#[when(expr = "the city runs until a shopper is walking to {string}")]
fn runs_until_walking(w: &mut SimWorld, name: String) {
    let id = w.city.building(&name);
    for _ in 0..8_640 {
        tick(w);
        if walking_to(w, id) {
            return;
        }
    }
    panic!("nobody walked to {name}");
}

#[then(expr = "a shopper is walking to {string}")]
fn is_walking_to(w: &mut SimWorld, name: String) {
    let id = w.city.building(&name);
    assert!(walking_to(w, id), "{:?}", w.city.customers.agents);
}

fn lost(w: &SimWorld, shop: sim_core::businesses::BusinessId) -> usize {
    w.city
        .reports
        .iter()
        .flat_map(|r| &r.lost)
        .filter(|l| l.shop == shop)
        .count()
}

#[when(expr = "the city runs until {string} has lost a sale")]
fn runs_until_lost(w: &mut SimWorld, name: String) {
    let id = w.city.building(&name);
    for _ in 0..8_640 {
        tick(w);
        if lost(w, id) > 0 {
            return;
        }
    }
    panic!("{name} never lost a sale");
}

#[then(regex = r#"^"([^"]+)" lost (\d+) sales?$"#)]
fn lost_sales(w: &mut SimWorld, name: String, n: usize) {
    let id = w.city.building(&name);
    assert_eq!(lost(w, id), n);
}

#[then(regex = r#"^"([^"]+)" sold (\d+) (\w+)$"#)]
fn sold(w: &mut SimWorld, name: String, n: u32, good: String) {
    let config = CityState::config(w);
    let goods = GoodsIndex::new(&config.businesses.goods);
    let id = w.city.building(&name);
    let total: u32 = w
        .city
        .reports
        .iter()
        .flat_map(|r| &r.sales)
        .filter(|s| s.shop == id && goods.name(s.good) == good)
        .map(|s| s.qty)
        .sum();
    assert_eq!(total, n);
}

#[then(expr = "{int} shoppers came out by day")]
fn came_out(w: &mut SimWorld, n: usize) {
    let day = w
        .city
        .spawned
        .iter()
        .filter(|c| c.wave == Wave::Day)
        .count();
    assert_eq!(day, n);
}

#[then(expr = "{int} of them came from home and {int} from the tube")]
fn from_home_and_tube(w: &mut SimWorld, home: usize, tube: usize) {
    let residents = w
        .city
        .spawned
        .iter()
        .filter(|c| matches!(c.origin, Origin::Resident(_)))
        .count();
    assert_eq!(residents, home);
    assert_eq!(w.city.spawned.len() - residents, tube);
}

#[then(expr = "{int} commuters and {int} tourists came out by day")]
fn commuters_tourists(w: &mut SimWorld, commuters: usize, tourists: usize) {
    let day = || w.city.spawned.iter().filter(|c| c.wave == Wave::Day);
    assert_eq!(
        day().filter(|c| c.origin == Origin::Commuter).count(),
        commuters
    );
    assert_eq!(
        day().filter(|c| c.origin == Origin::Tourist).count(),
        tourists
    );
}

fn check_exits(w: &mut SimWorld, which: &[sim_core::customers::Customer]) {
    let config = CityState::config(w);
    let map = CityState::map(w);
    let g = geometry(w);
    let year = CityState::clock(w).year();
    let stations: BTreeSet<_> = station_cells(&config, &map, year, g).into_iter().collect();
    let doors = front_doors(&w.city.businesses, &map, g);
    assert!(!which.is_empty(), "no shoppers to check");
    for c in which {
        match c.origin {
            Origin::Resident(r) => {
                let home = w.city.population.residential.residents[&r].home;
                assert_eq!(c.exit, doors[&home], "{c:?}");
            }
            Origin::Commuter | Origin::Tourist => assert!(stations.contains(&c.exit), "{c:?}"),
        }
    }
}

#[then("every resident set out from home and every visitor from a tube station")]
fn set_out(w: &mut SimWorld) {
    let all = w.city.spawned.clone();
    check_exits(w, &all);
}

#[then("some day shoppers are still about")]
fn still_about(w: &mut SimWorld) {
    assert!(w.city.customers.agents.iter().any(|c| c.wave == Wave::Day));
}

#[then("nobody is on the map")]
fn nobody(w: &mut SimWorld) {
    assert!(
        w.city.customers.agents.is_empty(),
        "{} left",
        w.city.customers.agents.len()
    );
}

#[then("every night shopper still about is heading home or to a tube station")]
fn heading_out(w: &mut SimWorld) {
    let about: Vec<_> = w.city.customers.agents.clone();
    assert!(about.iter().all(|c| c.wave == Wave::Night));
    assert!(
        about.iter().all(|c| c.activity == Activity::Leaving),
        "{about:?}"
    );
    if !about.is_empty() {
        check_exits(w, &about);
    }
    // Some residents and some visitors came out at night, and nobody waits to appear.
    let night: Vec<_> = w
        .city
        .spawned
        .iter()
        .filter(|c| c.wave == Wave::Night)
        .cloned()
        .collect();
    assert!(
        night
            .iter()
            .any(|c| matches!(c.origin, Origin::Resident(_)))
    );
    assert!(
        night
            .iter()
            .any(|c| !matches!(c.origin, Origin::Resident(_)))
    );
}

#[then(
    regex = r"^the price factor for a (working|middle|upper) shopper at (\d+)% of the reference price is ([\d.]+)$"
)]
fn price_factor(w: &mut SimWorld, c: String, pct: f64, expected: f64) {
    let config = CityState::config(w);
    let k = scoring::k_class(&config.customers, class(&c));
    let got = scoring::price_factor(k, pct, 100.0);
    assert!((got - expected).abs() < 5e-5, "{got} vs {expected}");
}

#[when(regex = r#"^(\d+) (working|middle|upper) shoppers at "([^"]+)" choose where to buy (\w+)$"#)]
fn choose(w: &mut SimWorld, n: u32, c: String, block: String, good: String) {
    let config = CityState::config(w);
    let map = CityState::map(w);
    let g = geometry(w);
    let goods = GoodsIndex::new(&config.businesses.goods);
    let good = goods.id(&good).unwrap();
    // Choose at midday so day shops are open.
    let mut clock = CityState::clock(w).clone();
    while clock.second_of_day() != 12 * 3600 {
        clock.advance();
    }
    let offers = shop_offers(&w.city.businesses, &map, &config, &clock, &goods, g);
    let at_block = CityState::block(w, &block);
    let at = g.centre(map.block(at_block).unwrap());
    let who = cohort(w, &c, "men", "30-49");
    w.city.picks.clear();
    w.city.pick_total = n;
    let mut rng = w.city.rng().clone();
    for _ in 0..n {
        if let Some(i) = choose_shop(
            who,
            at,
            sim_core::Money::from_pounds(1_000),
            good,
            &offers,
            &w.city.customers.reputation,
            &config.customers,
            g,
            &mut rng,
        ) {
            *w.city.picks.entry(offers[i].shop).or_insert(0) += 1;
        }
    }
    w.city.rng = Some(rng);
}

fn picks(w: &SimWorld, name: &str) -> u32 {
    let id = w.city.building(name);
    w.city.picks.get(&id).copied().unwrap_or(0)
}

#[then(expr = "both {string} and {string} were chosen")]
fn both_chosen(w: &mut SimWorld, a: String, b: String) {
    assert!(picks(w, &a) > 0 && picks(w, &b) > 0, "{:?}", w.city.picks);
}

#[then(expr = "{string} was chosen more often than {string}")]
fn chosen_more(w: &mut SimWorld, a: String, b: String) {
    assert!(picks(w, &a) > picks(w, &b), "{:?}", w.city.picks);
}

#[then(expr = "{string} was never chosen")]
fn never_chosen(w: &mut SimWorld, a: String) {
    assert_eq!(picks(w, &a), 0);
}

#[then(regex = r#"^"([^"]+)" was chosen by at (least|most) (\d+)% of them$"#)]
fn chosen_share(w: &mut SimWorld, a: String, bound: String, pct: u32) {
    let got = picks(w, &a) * 100;
    let limit = pct * w.city.pick_total;
    if bound == "least" {
        assert!(got >= limit, "{:?}", w.city.picks);
    } else {
        assert!(got <= limit, "{:?}", w.city.picks);
    }
}
