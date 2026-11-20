//! Steps for `features/land/`: lots and frontage (R22), leases and land rent (R9, R33), the
//! city's starting apartment blocks (R28). Game-level steps share the game in
//! `steps::auctions`.

use cucumber::{gherkin::Step, given, then, when};
use sim_core::World;
use sim_core::economy::AccountId;
use sim_core::economy::settlement::rent_bill;
use sim_core::land::{BlockId, Lot, PlacementError, Position, Size};

use crate::SimWorld;
use crate::steps::auctions::{account, game, game_mut};

/// The lot under test.
#[derive(Debug, Default)]
pub struct LandState {
    lot: Option<Lot>,
}

fn size(text: &str) -> Size {
    let (w, d) = text.split_once('x').unwrap();
    Size {
        w: w.parse().unwrap(),
        d: d.parse().unwrap(),
    }
}

fn lot(w: &mut SimWorld) -> &mut Lot {
    w.land
        .lot
        .as_mut()
        .expect("no lot: add 'Given an empty lot'")
}

#[given("an empty lot")]
fn empty_lot(w: &mut SimWorld) {
    let config = w.config.load_ok();
    w.land.lot = Some(Lot::of_size(config.game.world.lot_size));
}

#[given(regex = r"^a (\d+x\d+) business already stands at (\d+),(\d+)$")]
fn already_stands(w: &mut SimWorld, s: String, x: u32, y: u32) {
    lot(w).place(size(&s), Position { x, y }).unwrap();
}

#[when(regex = r"^(\d+x\d+) businesses are built at:$")]
fn built_at(w: &mut SimWorld, s: String, step: &Step) {
    let table = step.table.as_ref().expect("a table of x, y");
    for row in table.rows.iter().skip(1) {
        let (x, y) = (row[0].parse().unwrap(), row[1].parse().unwrap());
        lot(w)
            .place(size(&s), Position { x, y })
            .unwrap_or_else(|e| panic!("{s} at {x},{y}: {e:?}"));
    }
}

#[then(expr = "the lot is {int} units wide and {int} units deep")]
fn lot_size(w: &mut SimWorld, width: u32, depth: u32) {
    assert_eq!((lot(w).width(), lot(w).depth()), (width, depth));
}

#[then(expr = "the lot holds {int} businesses")]
fn holds(w: &mut SimWorld, n: usize) {
    assert_eq!(lot(w).occupied().len(), n);
}

#[then(regex = r"^a (\d+x\d+) business at (\d+),(\d+) (can be built|is refused: .+)$")]
fn placement(w: &mut SimWorld, s: String, x: u32, y: u32, result: String) {
    let actual = lot(w).can_place(size(&s), Position { x, y });
    match result.as_str() {
        "can be built" => assert!(actual.is_ok(), "{s} at {x},{y}: {actual:?}"),
        "is refused: no street frontage" => assert_eq!(actual, Err(PlacementError::NoFrontage)),
        "is refused: outside the lot" => assert_eq!(actual, Err(PlacementError::OutsideLot)),
        "is refused: overlaps another business" => {
            assert!(
                matches!(actual, Err(PlacementError::Overlaps(_))),
                "{actual:?}"
            );
        }
        other => panic!("unknown result {other:?}"),
    }
}

/// A block id by district name.
pub fn block_named(world: &World, name: &str) -> BlockId {
    let i = world
        .map()
        .blocks
        .iter()
        .position(|b| b.name == name)
        .unwrap_or_else(|| panic!("no block named {name:?}"));
    BlockId(u32::try_from(i).unwrap())
}

#[then(regex = r"^player (\d+) holds the lease on (.+) with (\d+) years left$")]
fn holds_lease(w: &mut SimWorld, player: u8, name: String, years: u32) {
    let world = game(w);
    let block = world
        .map()
        .block(block_named(world, unquote(&name)))
        .unwrap();
    let lease = block
        .lease
        .as_ref()
        .unwrap_or_else(|| panic!("{name} is unleased"));
    assert_eq!(lease.holder, account(player));
    assert_eq!(lease.remaining_years(world.clock().year()), years);
}

#[then(regex = r"^(.+) has no leaseholder$")]
fn unleased(w: &mut SimWorld, name: String) {
    let world = game(w);
    let block = world
        .map()
        .block(block_named(world, unquote(&name)))
        .unwrap();
    assert!(block.lease.is_none(), "{name}: {:?}", block.lease);
}

#[then(expr = "the year is {int} in the game")]
fn year_in_game(w: &mut SimWorld, year: i32) {
    assert_eq!(game(w).clock().year(), year);
}

#[when(regex = r"^(\d+) game days? go by$")]
fn runs_days(w: &mut SimWorld, days: u64) {
    let per_day = u64::from(game(w).clock().rules().ticks_per_day());
    let world = game_mut(w);
    for _ in 0..days * per_day {
        world.tick();
    }
}

#[when("the game runs to the next midnight")]
fn next_midnight(w: &mut SimWorld) {
    let world = game_mut(w);
    loop {
        if world.tick().midnight {
            break;
        }
    }
}

#[when(regex = r"^the game runs to (\d{4})$")]
fn runs_to_year(w: &mut SimWorld, year: i32) {
    let world = game_mut(w);
    while world.clock().year() < year {
        world.tick();
    }
}

fn unquote(s: &str) -> &str {
    s.trim_matches('"')
}

/// The city apartment block (or any apartment block) on a block.
fn apartments_on(world: &World, name: &str) -> (sim_core::businesses::BusinessId, AccountId) {
    let block = block_named(world, name);
    let type_id = &world.config().map.city_apartments.business_type;
    world
        .businesses()
        .by_id
        .iter()
        .find_map(|(id, b)| (b.block == block && &b.type_id == type_id).then_some((*id, b.owner)))
        .unwrap_or_else(|| panic!("no apartment block on {name}"))
}

/// The apartment block's id on a district, for auction steps.
pub fn apartment_block(world: &World, name: &str) -> sim_core::businesses::BusinessId {
    apartments_on(world, name).0
}

#[then(regex = r#"^(the city|player \d+) owns the apartment block on "(.+)"$"#)]
fn owns_apartments(w: &mut SimWorld, who: String, name: String) {
    let (_, owner) = apartments_on(game(w), &name);
    assert_eq!(owner, parse_who(&who));
}

/// `the city` or `player N`.
pub fn parse_who(who: &str) -> AccountId {
    match who.strip_prefix("player ") {
        Some(n) => account(n.parse().unwrap()),
        None => AccountId::City,
    }
}

#[then(
    regex = r#"^the land rent from the apartment block on "(.+)" goes to (the city|player \d+)$"#
)]
fn rent_goes_to(w: &mut SimWorld, name: String, who: String) {
    let world = game(w);
    let expected = parse_who(&who);
    let block = block_named(world, &name);
    assert_eq!(world.map().rent_payee(block), expected);
    let (id, owner) = apartments_on(world, &name);
    let b = &world.businesses().by_id[&id];
    let bill = rent_bill(b, world.map(), world.config()).unwrap();
    assert_eq!((bill.from, bill.to), (owner, expected));
    assert!(bill.amount > sim_core::Money::ZERO);
}

#[then("every Low-tier set has exactly one apartment block owned by the city")]
fn one_per_low_set(w: &mut SimWorld) {
    let world = game(w);
    let map = world.map();
    for (set, blocks) in map.sets() {
        let low = blocks
            .iter()
            .all(|b| map.block(*b).unwrap().tier == sim_core::land::Tier::Low);
        if !low {
            continue;
        }
        let count = world
            .businesses()
            .by_id
            .values()
            .filter(|b| b.owner == AccountId::City && blocks.contains(&b.block))
            .count();
        assert_eq!(count, 1, "set {set}");
    }
}

#[then("no other set has a city apartment block")]
fn none_elsewhere(w: &mut SimWorld) {
    let world = game(w);
    let map = world.map();
    for b in world.businesses().by_id.values() {
        assert_eq!(map.block(b.block).unwrap().tier, sim_core::land::Tier::Low);
    }
}
