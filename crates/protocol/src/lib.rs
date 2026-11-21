//! OpenTycoon wire protocol v1: the messages between the server and the Godot client for the
//! game inspector (spec §10d, R88). The contract, field by field, is
//! `docs/inspector-protocol.md`; change both together and bump [`PROTOCOL_VERSION`].
//!
//! WebSocket text frames, one JSON object per frame, tagged by `"type"` in snake case. Clients
//! send [`ClientMessage`]s (commands only: the server is authoritative, R40); the server sends
//! [`ServerMessage`]s (one snapshot, then deltas). Money is integer pence; positions are metres
//! on the ground plane (`x` east, `z` south, origin at the board's north-west corner).

use serde::{Deserialize, Serialize};

/// Protocol version; bump on any breaking change.
pub const PROTOCOL_VERSION: u32 = 1;

/// Seats in a new game when `new_game` leaves `players` out.
pub const DEFAULT_PLAYERS: u8 = 4;

fn default_players() -> u8 {
    DEFAULT_PLAYERS
}

/// Client → server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// First message on a connection; the server replies `welcome` or `error`.
    Hello {
        /// Client's protocol version.
        protocol: u32,
        /// The one-time token the server was started with (single player).
        token: String,
    },
    /// Starts a game in which every seat is an AI and the viewer is a spectator.
    NewGame {
        /// Game seed.
        seed: u64,
        /// Path to an `economy.yaml` to play instead of the shipped one; `None` = shipped.
        #[serde(default)]
        economy: Option<String>,
        /// Seats (default [`DEFAULT_PLAYERS`]).
        #[serde(default = "default_players")]
        players: u8,
    },
    /// `0` pauses; `1`–`4` are the speed steps in `config/server.yaml`.
    SetSpeed {
        /// Speed step.
        speed: u8,
    },
    /// Ends the game; the server closes the connection.
    Leave,
}

/// Server → client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Reply to [`ClientMessage::Hello`].
    Welcome {
        /// Server's protocol version.
        protocol: u32,
    },
    /// The whole visible game, once, after `new_game`.
    Snapshot(Snapshot),
    /// What changed since the last snapshot or delta.
    Delta(Delta),
    /// The final whistle.
    GameOver(GameOver),
    /// Something the client sent was refused.
    Error {
        /// Why.
        message: String,
    },
}

/// The game clock as the client shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Clock {
    /// In-game year.
    pub year: i32,
    /// Game days since the start.
    pub day: u32,
    /// Time of day, `"HH:MM"`.
    pub time: String,
    /// Current speed step (`0` paused).
    pub speed: u8,
}

/// An axis-aligned rectangle on the ground plane, in metres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    /// West edge.
    pub x: f64,
    /// North edge.
    pub z: f64,
    /// Width (east–west).
    pub w: f64,
    /// Depth (north–south).
    pub d: f64,
}

/// One block (lot) of the board, where it stands and what it is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoardBlock {
    /// Config id, e.g. `old_kent_road`.
    pub key: String,
    /// Display name, e.g. `Old Kent Road`.
    pub name: String,
    /// Display name of its set (district); empty for a block outside any set.
    pub district: String,
    /// Set id; empty for a block outside any set.
    pub set: String,
    /// Column.
    pub col: u32,
    /// Row.
    pub row: u32,
    /// West edge of the lot, metres.
    pub x: f64,
    /// North edge of the lot, metres.
    pub z: f64,
    /// Lot width, metres.
    pub w: f64,
    /// Lot depth, metres.
    pub d: f64,
    /// True for leasable, buildable land.
    pub buildable: bool,
}

/// The board: blocks, the river, its bridges and the streets. Sent once, in the snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Board {
    /// Columns of blocks.
    pub cols: u32,
    /// Rows of blocks.
    pub rows: u32,
    /// Whole board width, metres.
    pub width_m: f64,
    /// Whole board depth, metres.
    pub depth_m: f64,
    /// Every block, row-major.
    pub blocks: Vec<BoardBlock>,
    /// Water: the river's road row away from the bridges.
    pub river: Vec<Rect>,
    /// Bridges over the river.
    pub bridges: Vec<Rect>,
    /// Streets: one rectangle per north–south and east–west road (water left out).
    pub streets: Vec<Rect>,
}

/// One player seat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Player {
    /// `"p0"`, `"p1"`, …
    pub id: String,
    /// Display name (the AI personality playing the seat).
    pub name: String,
    /// Cash balance, pence.
    pub cash: i64,
    /// Net worth at the last midnight (cash before the first), pence.
    pub net_worth: i64,
    /// Out of the game (R4).
    pub bankrupt: bool,
}

/// A lease on a lot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LotLease {
    /// Whole years left on the lease.
    pub years_left: u32,
}

/// The building on a lot (the largest, when there are several).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LotBuilding {
    /// `"shop"`, `"apartments"`, `"hotel"`, `"online"` or `"infrastructure"`.
    pub kind: String,
    /// Its business type's display name.
    pub name: String,
    /// Height in lot units (one unit is drawn as one storey).
    pub floors: u32,
    /// Flats in it (apartments only, else 0).
    pub flats: u32,
}

/// The business in the lot's building.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LotBusiness {
    /// Display name of its type.
    pub name: String,
    /// Business type id from `businesses.yaml`.
    pub kind: String,
    /// Owned and run by the city.
    pub city_run: bool,
}

/// One lot's state for the lot card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lot {
    /// Block key.
    pub key: String,
    /// `"p0"`…, `"city"`, or `"unowned"`.
    pub owner: String,
    /// Land value now, pence.
    pub land_value: i64,
    /// The lease, if any.
    pub lease: Option<LotLease>,
    /// The (largest) building, if any.
    pub building: Option<LotBuilding>,
    /// Its business, if any.
    pub business: Option<LotBusiness>,
    /// Buildings on the lot.
    pub buildings: u32,
    /// Sales and room nights over the last completed game day, every building, pence.
    pub takings_per_day: i64,
    /// Daily running cost (land rent, utilities, standing order), every building, pence.
    pub costs_per_day: i64,
    /// `takings + rent − costs`, pence.
    pub profit_per_day: i64,
    /// Residents' rent the lot's flats collect per day, pence.
    pub rent_per_day: i64,
    /// Its lease can be bid for (unleased land, or on the block now).
    pub open_for_bids: bool,
    /// The last price it sold for at auction, pence.
    pub last_sale_price: Option<i64>,
}

/// What an auction sells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuctionItemKind {
    /// A block's lease.
    Land,
    /// A business.
    Business,
    /// A utility block.
    Utility,
}

/// The live auction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Auction {
    /// The block it is on.
    pub block: String,
    /// What is sold.
    pub item: AuctionItemKind,
    /// The high bid (the reserve before anyone bids), pence.
    pub high_bid: i64,
    /// The high bidder, if anyone has bid.
    pub high_bidder: Option<String>,
    /// Real seconds (at 1×) until bidding opens, or until the turn runs out.
    pub seconds_left: u32,
    /// `0` in the countdown, then the round of bidding, from 1.
    pub round: u32,
}

/// What a bid event is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BidKind {
    /// An auction opened (with the opening bid, or the reserve).
    Open,
    /// A higher bid.
    Raise,
    /// A player passed (or let the turn timer run out).
    Pass,
    /// The auction closed with a winner.
    Won,
    /// The auction closed with no bid, or the winner could not pay.
    Unsold,
}

/// One event from the auction house.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bid {
    /// Tick it happened on.
    pub tick: u64,
    /// The block.
    pub block: String,
    /// Who, if a player.
    pub player: Option<String>,
    /// The bid or price, pence.
    pub amount: i64,
    /// What happened.
    pub kind: BidKind,
}

/// The whole visible game.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Ticks since the start.
    pub tick: u64,
    /// Clock.
    pub clock: Clock,
    /// The board.
    pub board: Board,
    /// Every seat.
    pub players: Vec<Player>,
    /// Every lot, one per block, in board order.
    pub lots: Vec<Lot>,
    /// The live auction.
    pub auction: Option<Auction>,
    /// Recent bid events, newest last.
    pub bids: Vec<Bid>,
    /// Walker positions `[x0, z0, x1, z1, …]`, metres.
    pub walkers: Vec<f32>,
}

/// What changed. `players` and `lots` hold only what changed (replace by `id` / `key`);
/// `bids` only new events; `auction` and `walkers` are always the full current value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Delta {
    /// Ticks since the start.
    pub tick: u64,
    /// Clock.
    pub clock: Clock,
    /// Changed seats.
    pub players: Vec<Player>,
    /// Changed lots.
    pub lots: Vec<Lot>,
    /// The live auction.
    pub auction: Option<Auction>,
    /// New bid events, oldest first.
    pub bids: Vec<Bid>,
    /// Walker positions `[x0, z0, x1, z1, …]`, metres.
    pub walkers: Vec<f32>,
}

/// The final whistle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameOver {
    /// `"p2"`, `"tie"` or `"none"`.
    pub winner: String,
    /// Every seat at the end.
    pub players: Vec<Player>,
}

/// `"p<n>"` for seat `n`.
pub fn player_id(seat: u8) -> String {
    format!("p{seat}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip_client(msg: &ClientMessage, json: &str) {
        assert_eq!(serde_json::to_string(msg).unwrap(), json);
        assert_eq!(&serde_json::from_str::<ClientMessage>(json).unwrap(), msg);
    }

    #[test]
    fn client_messages_round_trip_as_tagged_json() {
        round_trip_client(
            &ClientMessage::Hello {
                protocol: PROTOCOL_VERSION,
                token: "t".into(),
            },
            r#"{"type":"hello","protocol":1,"token":"t"}"#,
        );
        round_trip_client(
            &ClientMessage::NewGame {
                seed: 42,
                economy: None,
                players: 4,
            },
            r#"{"type":"new_game","seed":42,"economy":null,"players":4}"#,
        );
        round_trip_client(
            &ClientMessage::SetSpeed { speed: 3 },
            r#"{"type":"set_speed","speed":3}"#,
        );
        round_trip_client(&ClientMessage::Leave, r#"{"type":"leave"}"#);
    }

    #[test]
    fn new_game_defaults_to_four_players_and_the_shipped_economy() {
        let msg: ClientMessage = serde_json::from_str(r#"{"type":"new_game","seed":7}"#).unwrap();
        assert_eq!(
            msg,
            ClientMessage::NewGame {
                seed: 7,
                economy: None,
                players: 4
            }
        );
    }

    #[test]
    fn unknown_types_are_refused() {
        assert!(serde_json::from_str::<ClientMessage>(r#"{"type":"build"}"#).is_err());
    }

    fn sample_lot() -> Lot {
        Lot {
            key: "mayfair".into(),
            owner: "p1".into(),
            land_value: 1_400_000,
            lease: Some(LotLease { years_left: 12 }),
            building: Some(LotBuilding {
                kind: "shop".into(),
                name: "Grocery".into(),
                floors: 2,
                flats: 0,
            }),
            business: Some(LotBusiness {
                name: "Grocery".into(),
                kind: "grocery".into(),
                city_run: false,
            }),
            buildings: 1,
            takings_per_day: 500,
            costs_per_day: 200,
            profit_per_day: 300,
            rent_per_day: 0,
            open_for_bids: false,
            last_sale_price: None,
        }
    }

    #[test]
    fn server_messages_round_trip_with_the_documented_fields() {
        let snapshot = ServerMessage::Snapshot(Snapshot {
            tick: 0,
            clock: Clock {
                year: 1930,
                day: 0,
                time: "06:00".into(),
                speed: 1,
            },
            board: Board {
                cols: 1,
                rows: 1,
                width_m: 44.0,
                depth_m: 44.0,
                blocks: vec![BoardBlock {
                    key: "mayfair".into(),
                    name: "Mayfair".into(),
                    district: "Mayfair & Belgravia".into(),
                    set: "green".into(),
                    col: 0,
                    row: 0,
                    x: 4.0,
                    z: 4.0,
                    w: 36.0,
                    d: 36.0,
                    buildable: true,
                }],
                river: vec![],
                bridges: vec![],
                streets: vec![Rect {
                    x: 0.0,
                    z: 0.0,
                    w: 44.0,
                    d: 4.0,
                }],
            },
            players: vec![Player {
                id: player_id(0),
                name: "The Grocer".into(),
                cash: 1,
                net_worth: 2,
                bankrupt: false,
            }],
            lots: vec![sample_lot()],
            auction: Some(Auction {
                block: "mayfair".into(),
                item: AuctionItemKind::Land,
                high_bid: 0,
                high_bidder: None,
                seconds_left: 120,
                round: 0,
            }),
            bids: vec![Bid {
                tick: 3,
                block: "mayfair".into(),
                player: Some("p1".into()),
                amount: 100,
                kind: BidKind::Open,
            }],
            walkers: vec![2.0, 6.0],
        });
        let json = serde_json::to_value(&snapshot).unwrap();
        assert_eq!(json["type"], "snapshot");
        for key in [
            "tick", "clock", "board", "players", "lots", "auction", "bids", "walkers",
        ] {
            assert!(json.get(key).is_some(), "snapshot has no {key}");
        }
        assert_eq!(json["lots"][0]["lease"]["years_left"], 12);
        assert_eq!(json["auction"]["item"], "land");
        assert_eq!(json["bids"][0]["kind"], "open");
        let back: ServerMessage = serde_json::from_value(json).unwrap();
        assert_eq!(back, snapshot);

        let mut lot = sample_lot();
        lot.lease = None;
        lot.building = None;
        lot.business = None;
        let json = serde_json::to_value(&lot).unwrap();
        assert!(json["lease"].is_null() && json["building"].is_null());
        assert!(json["business"].is_null() && json["last_sale_price"].is_null());

        let over = ServerMessage::GameOver(GameOver {
            winner: "tie".into(),
            players: vec![],
        });
        assert_eq!(
            serde_json::to_string(&over).unwrap(),
            r#"{"type":"game_over","winner":"tie","players":[]}"#
        );
        let err = ServerMessage::Error {
            message: "no".into(),
        };
        assert_eq!(
            serde_json::to_string(&err).unwrap(),
            r#"{"type":"error","message":"no"}"#
        );
        assert_eq!(
            serde_json::to_string(&ServerMessage::Welcome { protocol: 1 }).unwrap(),
            r#"{"type":"welcome","protocol":1}"#
        );
    }

    #[test]
    fn a_delta_has_no_board() {
        let delta = ServerMessage::Delta(Delta {
            tick: 9,
            clock: Clock {
                year: 1930,
                day: 0,
                time: "00:01".into(),
                speed: 2,
            },
            players: vec![],
            lots: vec![],
            auction: None,
            bids: vec![],
            walkers: vec![],
        });
        let json = serde_json::to_value(&delta).unwrap();
        assert_eq!(json["type"], "delta");
        assert!(json.get("board").is_none());
        assert!(json["auction"].is_null());
        assert_eq!(
            serde_json::from_value::<ServerMessage>(json).unwrap(),
            delta
        );
    }
}
