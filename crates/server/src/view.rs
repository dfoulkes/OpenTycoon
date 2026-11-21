//! What the client sees: pure functions from a [`World`] to protocol shapes
//! (`docs/inspector-protocol.md`). They only read the world, so they can never change a game,
//! and they need no socket to test.
//!
//! Geometry follows the walking grid (`sim_core::customers::movement`): each block is a
//! `lot_w × lot_d` lot inside a one-cell road ring, one cell is `metres_per_lot_unit` metres,
//! and the origin is the north-west corner of cell `(0, 0)`.
#![allow(
    clippy::cast_precision_loss,
    reason = "board coordinates are small integers (cells < 2^24), exact in f32 and f64"
)]

use std::collections::{BTreeMap, BTreeSet};

use protocol::{
    Auction, AuctionItemKind, AuctionSeat, Board, BoardBlock, Clock, GoodRow, Lot, LotBuilding,
    LotBusiness, LotLease, Player, Rect, SeatStatus,
};
use sim_core::auctions::{AuctionItem, AuctionOutcome, AuctionPhase};
use sim_core::businesses::{Business, BusinessId, Hours};
use sim_core::config::businesses::Channel;
use sim_core::config::economy::LandValueMethod;
use sim_core::economy::net_worth::{lease_land_value, market_land_price};
use sim_core::economy::{AccountId, PlayerId};
use sim_core::land::{Block, BlockId, BlockKind, volume};
use sim_core::population::residential::flats;
use sim_core::world::PlayerStatus;
use sim_core::{Money, Rate, World};

/// `"p<n>"`, `"city"` for the city (and the shoppers' purse, which never owns anything).
pub fn account_id(account: AccountId) -> String {
    match account {
        AccountId::Player(p) => protocol::player_id(p.0),
        AccountId::City | AccountId::Shoppers => "city".to_owned(),
    }
}

/// The board's cell pitch and scale: `(pitch_x, pitch_y, lot_w, lot_d, metres per cell)`.
fn grid(world: &World) -> (u32, u32, u32, u32, f64) {
    let [lot_w, lot_d] = world.config().game.world.lot_size;
    let m = f64::from(world.config().game.world.metres_per_lot_unit);
    (lot_w + 1, lot_d + 1, lot_w, lot_d, m)
}

/// The clock at `speed`.
pub fn clock(world: &World, speed: u8) -> Clock {
    let c = world.clock();
    let (h, m, _) = c.time_of_day();
    Clock {
        year: c.year(),
        day: c.day(),
        time: format!("{h:02}:{m:02}"),
        speed,
    }
}

/// The board: every block with its lot rectangle, the river, its bridges and the streets.
pub fn board(world: &World) -> Board {
    let map = world.map();
    let (px, py, lot_w, lot_d, m) = grid(world);
    let cells_x = map.width * px + 1;
    let cells_y = map.height * py + 1;
    let (width_m, depth_m) = (f64::from(cells_x) * m, f64::from(cells_y) * m);
    let sets = &world.config().map.sets;
    let blocks = map
        .blocks
        .iter()
        .map(|b| {
            let set = b.set.clone().unwrap_or_default();
            BoardBlock {
                key: b.key.clone(),
                name: b.name.clone(),
                district: sets.get(&set).map(|s| s.name.clone()).unwrap_or_default(),
                set,
                col: b.col,
                row: b.row,
                x: f64::from(b.col * px + 1) * m,
                z: f64::from(b.row * py + 1) * m,
                w: f64::from(lot_w) * m,
                d: f64::from(lot_d) * m,
                buildable: b.kind == BlockKind::Buildable,
            }
        })
        .collect();
    // The river takes the road row south of `south_of_row`, except the bridge cells, which
    // carry the north–south road on the west side of their column (§4.1, R85).
    let river_row = map.river.as_ref().map(|r| r.south_of_row + 1);
    let bridge_cols: Vec<u32> = map
        .river
        .as_ref()
        .map(|r| r.bridges.clone())
        .unwrap_or_default();
    let cell = |x: u32, y: u32, w: u32, d: u32| Rect {
        x: f64::from(x) * m,
        z: f64::from(y) * m,
        w: f64::from(w) * m,
        d: f64::from(d) * m,
    };
    let mut streets = Vec::new();
    let mut river = Vec::new();
    let mut bridges = Vec::new();
    // East–west roads, one per junction row; the river's row is water.
    for j in 0..=map.height {
        if river_row == Some(j) {
            let y = j * py;
            let mut from = 0;
            let mut cols = bridge_cols.clone();
            cols.sort_unstable();
            for c in cols {
                let x = c * px;
                if x > from {
                    river.push(cell(from, y, x - from, 1));
                }
                bridges.push(cell(x, y, 1, 1));
                from = x + 1;
            }
            if cells_x > from {
                river.push(cell(from, y, cells_x - from, 1));
            }
        } else {
            streets.push(cell(0, j * py, cells_x, 1));
        }
    }
    // North–south roads, one per junction column, between the east–west roads (so no cell is
    // drawn twice); the stretch across the river row is a bridge or water, never a street.
    for i in 0..=map.width {
        for j in 0..map.height {
            streets.push(cell(i * px, j * py + 1, 1, py - 1));
        }
    }
    Board {
        cols: map.width,
        rows: map.height,
        width_m,
        depth_m,
        blocks,
        river,
        bridges,
        streets,
    }
}

/// Every seat, in seat order, named after the AI personality playing it, with the takings,
/// rent and running costs of the buildings it owns (`takings`: the last completed day's).
pub fn players(world: &World, takings: &DayTakings) -> Vec<Player> {
    let config = world.config();
    let year = world.clock().year();
    let rent: BTreeMap<BusinessId, Money> = world
        .rent_due()
        .into_iter()
        .map(|d| (d.building, d.amount))
        .collect();
    world
        .players()
        .iter()
        .map(|(p, status)| {
            let me = AccountId::Player(*p);
            let cash = world.ledger().balance(me);
            let (mut income, mut costs) = (Money::ZERO, Money::ZERO);
            for (id, _) in world.businesses().owned_by(me) {
                income += takings.takings.get(&id).copied().unwrap_or(Money::ZERO)
                    + rent.get(&id).copied().unwrap_or(Money::ZERO);
                costs += world
                    .businesses()
                    .running_cost(id, config, world.map(), year)
                    .map_or(Money::ZERO, |c| c.total());
            }
            Player {
                id: protocol::player_id(p.0),
                name: seat_name(world, *p),
                cash: cash.pence(),
                net_worth: world.net_worth().get(*p).unwrap_or(cash).pence(),
                bankrupt: *status == PlayerStatus::Bankrupt,
                costs_yesterday: costs.pence(),
                profit_yesterday: (income - costs).pence(),
            }
        })
        .collect()
}

/// The display name of whoever plays `seat`.
pub fn seat_name(world: &World, seat: PlayerId) -> String {
    world
        .ai_seats()
        .iter()
        .find(|s| s.player == seat)
        .and_then(|s| world.config().ai.personalities.get(&s.personality))
        .map_or_else(|| format!("Player {}", seat.0 + 1), |p| p.name.clone())
}

/// Highest net worth among the solvent players: `"p<n>"`, `"tie"` or `"none"`.
pub fn winner(players: &[Player]) -> String {
    let solvent = || players.iter().filter(|p| !p.bankrupt);
    let Some(best) = solvent().map(|p| p.net_worth).max() else {
        return "none".to_owned();
    };
    let mut leaders = solvent().filter(|p| p.net_worth == best);
    match (leaders.next(), leaders.next()) {
        (Some(p), None) => p.id.clone(),
        _ => "tie".to_owned(),
    }
}

/// The block an auction item stands on.
pub fn item_block(world: &World, item: AuctionItem) -> Option<BlockId> {
    match item {
        AuctionItem::Land(b) => Some(b),
        AuctionItem::Business(id) => world.businesses().get(id).map(|b| b.block),
        AuctionItem::Utility(kind) => world
            .map()
            .blocks
            .iter()
            .position(|b| b.utility == Some(kind))
            .and_then(|i| u32::try_from(i).ok())
            .map(BlockId),
    }
}

/// A block's key, or empty if there is no such block.
pub fn block_key(world: &World, block: Option<BlockId>) -> String {
    block
        .and_then(|b| world.map().block(b))
        .map(|b| b.key.clone())
        .unwrap_or_default()
}

/// Real seconds (at 1×) that `ticks` take, rounded up.
pub fn real_seconds(world: &World, ticks: u64) -> u32 {
    let rules = world.clock().rules();
    let per_day = u128::from(rules.ticks_per_day()).max(1) * 1_000;
    let ms = u128::from(ticks) * u128::from(rules.real_millis_per_day());
    u32::try_from(ms.div_ceil(per_day)).unwrap_or(u32::MAX)
}

/// What the inspector remembers of the live auction's room (the auction house keeps only the
/// leader and the high bid).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuctionMemo {
    /// The bidding round (0 in the countdown).
    pub round: u32,
    /// Seats that passed this round (R8: a pass counts for one round).
    pub passed: BTreeSet<u8>,
    /// Each seat's last bid in this auction.
    pub last_bid: BTreeMap<u8, Money>,
    /// The opening bid, or the reserve when the auction opened without one.
    pub opening_bid: Money,
}

/// The live auction and its room: one seat per player, in seat order.
pub fn auction(world: &World, memo: &AuctionMemo) -> Option<Auction> {
    let a = world.auctions().current()?;
    let now = world.clock().tick();
    let (ends, turn) = match a.phase {
        AuctionPhase::Countdown { starts_at_tick } => (starts_at_tick, None),
        AuctionPhase::Bidding {
            turn,
            turn_ends_at_tick,
        } => (turn_ends_at_tick, Some(turn)),
    };
    let seats = world
        .players()
        .iter()
        .map(|(p, status)| {
            let state = if *status == PlayerStatus::Bankrupt || a.seller == AccountId::Player(*p) {
                SeatStatus::Out
            } else if turn == Some(*p) {
                SeatStatus::ToAct
            } else if a.leader == Some(*p) {
                SeatStatus::Raised
            } else if memo.passed.contains(&p.0) {
                SeatStatus::Passed
            } else {
                SeatStatus::Waiting
            };
            AuctionSeat {
                player: protocol::player_id(p.0),
                status: state,
                last_bid: memo.last_bid.get(&p.0).map(|m| m.pence()),
            }
        })
        .collect();
    Some(Auction {
        block: block_key(world, item_block(world, a.item)),
        item: match a.item {
            AuctionItem::Land(_) => AuctionItemKind::Land,
            AuctionItem::Business(_) => AuctionItemKind::Business,
            AuctionItem::Utility(_) => AuctionItemKind::Utility,
        },
        high_bid: a.high_bid.pence(),
        high_bidder: a.leader.map(|p| protocol::player_id(p.0)),
        seconds_left: real_seconds(world, ends.saturating_sub(now)),
        round: match a.phase {
            AuctionPhase::Countdown { .. } => 0,
            AuctionPhase::Bidding { .. } => memo.round.max(1),
        },
        turn: turn.map(|p| protocol::player_id(p.0)),
        opener: a
            .opener
            .map_or_else(|| account_id(a.seller), |p| protocol::player_id(p.0)),
        opening_bid: memo.opening_bid.pence(),
        seats,
    })
}

/// Walker positions `[x0, z0, …]` in metres (the centre of each walker's cell), at most `max`
/// walkers, in the customers' own order.
pub fn walkers(world: &World, max: u32) -> Vec<f32> {
    let m = f64::from(world.config().game.world.metres_per_lot_unit);
    let max = usize::try_from(max).unwrap_or(usize::MAX);
    let mut out = Vec::with_capacity(max.min(4_096) * 2);
    for c in world.customers().on_map().take(max) {
        #[allow(
            clippy::cast_possible_truncation,
            reason = "metres on a board a few km wide; f32 is what the client draws with"
        )]
        {
            out.push(((f64::from(c.at.x) + 0.5) * m) as f32);
            out.push(((f64::from(c.at.y) + 0.5) * m) as f32);
        }
    }
    out
}

/// What the world forgets at midnight and the lot card shows: each business's takings and
/// units sold per good over the last completed game day (the session keeps them).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DayTakings {
    /// Takings per business.
    pub takings: BTreeMap<BusinessId, Money>,
    /// Units sold per business and good.
    pub sold: BTreeMap<BusinessId, BTreeMap<String, i64>>,
}

/// The shop table of one business: one row per good it sells, in good id order.
pub fn goods(world: &World, id: BusinessId, b: &Business, day: &DayTakings) -> Vec<GoodRow> {
    let catalogue = &world.config().businesses.goods;
    let yesterday = day.sold.get(&id);
    b.prices
        .iter()
        .map(|(good, rate)| {
            let g = catalogue.get(good);
            let get = |m: &BTreeMap<String, i64>| m.get(good).copied().unwrap_or(0);
            GoodRow {
                good: good.clone(),
                cost: g.map_or(0, |g| g.purchase_price.money().pence()),
                price: g.map_or(0, |g| rate.apply(g.ref_price.money()).pence()),
                sold_yesterday: yesterday.map_or(0, get),
                order: get(&b.standing_order),
                stock: b.stock_of(good),
                sold_today: get(&b.sold_today),
            }
        })
        .collect()
}

/// A building's kind for the client.
fn building_kind(world: &World, b: &Business) -> &'static str {
    let config = world.config();
    if b.type_id == config.customers.residential.building_type {
        "apartments"
    } else if sim_core::hotels::is_hotel(config, &b.type_id) || b.hours == Hours::Hotel {
        "hotel"
    } else {
        match config.businesses.types.get(&b.type_id).map(|t| t.channel) {
            Some(Channel::Online) => "online",
            Some(Channel::Infrastructure) => "infrastructure",
            _ => "shop",
        }
    }
}

/// A block's land value now: its lease's under `economy.yaml` `land.value_method`, or the
/// market price (the base price for `book_decay`) when unleased (§3).
fn land_value_now(world: &World, block: &Block, factors: &[Rate]) -> Money {
    let method = world.config().economy.land.value_method;
    let market = match method {
        LandValueMethod::BookDecay => block.land_base_price,
        _ => market_land_price(block.land_base_price, factors),
    };
    let year = world.clock().year();
    block.lease.as_ref().map_or(market, |l| {
        lease_land_value(
            method,
            l.last_price,
            market,
            l.remaining_years(year),
            l.term_years,
        )
    })
}

/// The lot card's building and business, for business `id`.
fn building_and_business(
    world: &World,
    id: BusinessId,
    b: &Business,
    day: &DayTakings,
) -> (LotBuilding, LotBusiness) {
    let name = world
        .config()
        .businesses
        .types
        .get(&b.type_id)
        .map_or_else(|| b.type_id.clone(), |t| t.name.clone());
    let kind = building_kind(world, b);
    let building = LotBuilding {
        kind: kind.to_owned(),
        name: name.clone(),
        floors: b.height,
        flats: if kind == "apartments" {
            flats(
                volume(b.height, b.footprint),
                world.config().economy.residential.flat_size,
            )
        } else {
            0
        },
    };
    let business = LotBusiness {
        goods: goods(world, id, b, day),
        name,
        kind: b.type_id.clone(),
        city_run: b.owner == AccountId::City,
        sold_today: b.sold_today.values().sum(),
        capacity: b
            .capacity_per_good
            .saturating_mul(i64::try_from(b.prices.len()).unwrap_or(i64::MAX)),
    };
    (building, business)
}

/// Every lot, one per block, in board order (§10d item 3, R88).
pub fn lots(world: &World, takings: &DayTakings) -> Vec<Lot> {
    let config = world.config();
    let map = world.map();
    let year = world.clock().year();
    let factors = match config.economy.land.value_method {
        LandValueMethod::BookDecay => Vec::new(),
        LandValueMethod::Market | LandValueMethod::MarketNoDecay => {
            world.events().land_value_factors(&config.events)
        }
    };
    let rent: BTreeMap<BusinessId, Money> = world
        .rent_due()
        .into_iter()
        .map(|d| (d.building, d.amount))
        .collect();
    let mut last_sale: BTreeMap<BlockId, Money> = BTreeMap::new();
    for outcome in world.auctions().history() {
        if let AuctionOutcome::Sold { item, price, .. } = outcome {
            if let Some(block) = item_block(world, *item) {
                last_sale.insert(block, *price);
            }
        }
    }
    let live = world
        .auctions()
        .current()
        .and_then(|a| item_block(world, a.item));
    let mut on_block: BTreeMap<BlockId, Vec<(BusinessId, &Business)>> = BTreeMap::new();
    for (id, b) in &world.businesses().by_id {
        on_block.entry(b.block).or_default().push((*id, b));
    }
    map.blocks
        .iter()
        .enumerate()
        .map(|(i, block)| {
            let id = BlockId(u32::try_from(i).unwrap_or(u32::MAX));
            let holder = match block.utility {
                Some(kind) => world.utilities().holders.get(&kind).copied(),
                None => block.lease.as_ref().map(|l| l.holder),
            };
            let owner = match (holder, block.kind) {
                (Some(a), _) => account_id(a),
                (None, BlockKind::Special) => "city".to_owned(),
                (None, _) => "unowned".to_owned(),
            };
            let land_value = land_value_now(world, block, &factors);
            let here = on_block.get(&id).map_or(&[][..], Vec::as_slice);
            let largest = here
                .iter()
                .max_by_key(|(bid, b)| (b.volume(), std::cmp::Reverse(*bid)));
            let (building, business) = largest
                .map(|(bid, b)| building_and_business(world, *bid, b, takings))
                .unzip();
            let sum = |f: &dyn Fn(BusinessId) -> Money| -> Money {
                here.iter().map(|(bid, _)| f(*bid)).sum()
            };
            let takings_per_day = sum(&|b| takings.takings.get(&b).copied().unwrap_or(Money::ZERO));
            let rent_per_day = sum(&|b| rent.get(&b).copied().unwrap_or(Money::ZERO));
            let costs_per_day = sum(&|b| {
                world
                    .businesses()
                    .running_cost(b, config, map, year)
                    .map_or(Money::ZERO, |c| c.total())
            });
            let leasable = matches!(block.kind, BlockKind::Buildable | BlockKind::Utility);
            Lot {
                key: block.key.clone(),
                owner,
                land_value: land_value.pence(),
                lease: block.lease.as_ref().map(|l| LotLease {
                    years_left: l.remaining_years(year),
                }),
                building,
                business,
                buildings: u32::try_from(here.len()).unwrap_or(u32::MAX),
                takings_per_day: takings_per_day.pence(),
                costs_per_day: costs_per_day.pence(),
                profit_per_day: (takings_per_day + rent_per_day - costs_per_day).pence(),
                rent_per_day: rent_per_day.pence(),
                open_for_bids: live == Some(id) || (leasable && holder.is_none()),
                last_sale_price: last_sale.get(&id).map(|p| p.pence()),
            }
        })
        .collect()
}
