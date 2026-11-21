//! What the client sees: pure functions from a [`World`] to protocol shapes
//! (`docs/inspector-protocol.md`). Stub: the scenarios in `features/inspector/` drive it.

use std::collections::BTreeMap;

use protocol::{Auction, Board, Clock, Lot, Player};
use sim_core::auctions::AuctionItem;
use sim_core::businesses::BusinessId;
use sim_core::economy::{AccountId, PlayerId};
use sim_core::land::BlockId;
use sim_core::{Money, World};

/// `"p<n>"` or `"city"`.
pub fn account_id(account: AccountId) -> String {
    match account {
        AccountId::Player(p) => protocol::player_id(p.0),
        AccountId::City | AccountId::Shoppers => "city".to_owned(),
    }
}

/// The clock at `speed`.
pub fn clock(world: &World, speed: u8) -> Clock {
    Clock {
        year: world.clock().year(),
        day: world.clock().day(),
        time: String::new(),
        speed,
    }
}

/// The board (stub: empty).
pub fn board(_world: &World) -> Board {
    Board {
        cols: 0,
        rows: 0,
        width_m: 0.0,
        depth_m: 0.0,
        blocks: Vec::new(),
        river: Vec::new(),
        bridges: Vec::new(),
        streets: Vec::new(),
    }
}

/// Every seat (stub: none).
pub fn players(_world: &World) -> Vec<Player> {
    Vec::new()
}

/// Seat name (stub).
pub fn seat_name(_world: &World, seat: PlayerId) -> String {
    format!("Player {}", seat.0 + 1)
}

/// Winner (stub).
pub fn winner(_players: &[Player]) -> String {
    "none".to_owned()
}

/// The block an auction item stands on (stub).
pub fn item_block(_world: &World, _item: AuctionItem) -> Option<BlockId> {
    None
}

/// A block's key (stub).
pub fn block_key(_world: &World, _block: Option<BlockId>) -> String {
    String::new()
}

/// The live auction (stub: none).
pub fn auction(_world: &World, _round: u32) -> Option<Auction> {
    None
}

/// Walker positions (stub: none).
pub fn walkers(_world: &World, _max: u32) -> Vec<f32> {
    Vec::new()
}

/// Each business's takings over the last completed game day.
pub type DayTakings = BTreeMap<BusinessId, Money>;

/// Every lot (stub: none).
pub fn lots(_world: &World, _takings: &DayTakings) -> Vec<Lot> {
    Vec::new()
}
