//! Steps for `features/customers/online.feature`: online demand from 1995 (§7.5; R15, R36), in
//! the city scene shared with `steps::customers`.

use cucumber::{given, then};
use sim_core::customers::online::brand_of;
use sim_core::customers::scoring::unit_to_rate;
use sim_core::customers::shops::GoodsIndex;
use sim_core::customers::{Sale, Wave, wave_budget};
use sim_core::economy::AccountId;
use sim_core::money::Money;
use sim_core::population::allowance;

use super::population::CityState;
use crate::SimWorld;
use crate::common;

#[given(expr = "the game starts in {int}")]
fn starts_in(w: &mut SimWorld, year: i32) {
    common::set_key(
        w.config.sources_mut(),
        "game.yaml",
        "/clock/start_year",
        &year.to_string(),
    );
}

#[given(regex = r#"^an online bakery "([^"]+)" on "([^"]+)" with (\d+) bread$"#)]
fn online_bakery(w: &mut SimWorld, name: String, block: String, qty: i64) {
    let id = CityState::add_building(w, &name, "web_bakery", &block, (3, 3, 4), AccountId::City);
    w.city
        .businesses
        .by_id
        .get_mut(&id)
        .unwrap()
        .stock
        .insert("bread".into(), qty);
}

#[given(regex = r#"^"([^"]+)" has a brand of ([\d.]+)$"#)]
fn set_brand(w: &mut SimWorld, name: String, brand: f64) {
    let id = w.city.building(&name);
    w.city.customers.brand.insert(id, unit_to_rate(brand));
}

#[then(regex = r#"^"([^"]+)" has a brand of ([\d.]+)$"#)]
fn brand_is(w: &mut SimWorld, name: String, brand: f64) {
    let config = CityState::config(w);
    let id = w.city.building(&name);
    assert_eq!(
        brand_of(&w.city.customers.brand, id, &config.customers),
        unit_to_rate(brand)
    );
}

fn bread_sales<'a>(w: &'a SimWorld, online: bool) -> impl Iterator<Item = &'a Sale> + 'a {
    w.city
        .reports
        .iter()
        .flat_map(move |r| if online { &r.online_sales } else { &r.sales })
}

fn bread_units(w: &mut SimWorld, online: bool, shop: Option<&str>) -> u32 {
    let config = CityState::config(w);
    let goods = GoodsIndex::new(&config.businesses.goods);
    let bread = goods.id("bread").unwrap();
    let shop = shop.map(|s| w.city.building(s));
    bread_sales(w, online)
        .filter(|s| s.good == bread && shop.is_none_or(|id| s.shop == id))
        .map(|s| s.qty)
        .sum()
}

#[then(regex = r"^between (\d+)% and (\d+)% of the bread sold was sold online$")]
fn online_share_between(w: &mut SimWorld, lo: u32, hi: u32) {
    let online = bread_units(w, true, None);
    let street = bread_units(w, false, None);
    let all = online + street;
    assert!(all > 0, "no bread sold");
    assert!(
        online * 100 >= lo * all && online * 100 <= hi * all,
        "{online} of {all} online"
    );
}

#[then(regex = r#"^"([^"]+)" sold and "([^"]+)" sold online (\d+) bread between them$"#)]
fn between_them(w: &mut SimWorld, street: String, online: String, n: u32) {
    let a = bread_units(w, false, Some(&street));
    let b = bread_units(w, true, Some(&online));
    assert_eq!(a + b, n, "{a} on the street + {b} online");
}

#[then(regex = r#"^"([^"]+)" sold between (\d+)% and (\d+)% fewer bread than the (\d+) wanted$"#)]
fn fewer_than_wanted(w: &mut SimWorld, shop: String, lo: u32, hi: u32, wanted: u32) {
    let sold = bread_units(w, false, Some(&shop));
    let drop = wanted - sold;
    assert!(
        drop * 100 >= lo * wanted && drop * 100 <= hi * wanted,
        "sold {sold} of {wanted}"
    );
}

#[then(regex = r#"^"([^"]+)" sold (\d+) bread online$"#)]
fn sold_online(w: &mut SimWorld, shop: String, n: u32) {
    assert_eq!(bread_units(w, true, Some(&shop)), n);
}

#[then("some bread was sold online")]
fn some_online(w: &mut SimWorld) {
    assert!(bread_units(w, true, None) > 0);
}

#[then(regex = r#"^"([^"]+)" sold more bread online than "([^"]+)"$"#)]
fn more_online(w: &mut SimWorld, a: String, b: String) {
    let x = bread_units(w, true, Some(&a));
    let y = bread_units(w, true, Some(&b));
    assert!(x > y, "{x} vs {y}");
}

#[then("no shopper spent more than their day budget")]
fn within_budget(w: &mut SimWorld) {
    let config = CityState::config(w);
    for c in w.city.spawned.iter().filter(|c| c.wave == Wave::Day) {
        let budget = wave_budget(
            &config.customers,
            allowance(&config.customers, c.cohort),
            Wave::Day,
        );
        let spent: Money = w
            .city
            .reports
            .iter()
            .flat_map(|r| r.sales.iter().chain(&r.online_sales))
            .filter(|s| s.customer == c.id)
            .map(|s| s.amount)
            .sum();
        assert!(spent <= budget, "{c:?} spent {spent} of {budget}");
    }
    assert!(
        w.city
            .customers
            .agents
            .iter()
            .all(|c| c.budget >= Money::ZERO)
    );
}
