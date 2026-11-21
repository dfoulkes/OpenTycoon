//! One inspector game: a [`World`] in which every seat is an AI, stepped a tick at a time and
//! turned into one `snapshot` then `delta`s (spec §10d, R88; `docs/inspector-protocol.md`).
//!
//! The session only reads the world between ticks, so a game watched here is the same game,
//! tick for tick, as `World` run on its own with the same config and seed (R40: the server is
//! the sim, the client only displays it). It keeps three things the world does not:
//!
//! - **the last completed day's takings** per business (the world resets them at midnight);
//! - **bid events**, by comparing the auction house before and after each tick (the house
//!   keeps the live auction and the outcomes, not every bid);
//! - **what the client last saw**, so a delta carries only the lots and players that changed.

use std::collections::VecDeque;

use protocol::{Bid, BidKind, Delta, GameOver, Lot, Player, Snapshot};
use sim_core::auctions::{Auction as SimAuction, AuctionOutcome, AuctionPhase};
use sim_core::clock::ClockEvents;
use sim_core::economy::PlayerId;
use sim_core::{Config, Money, World};

use crate::config::ServerConfig;
use crate::view::{self, DayTakings};

/// Why a game could not start.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SessionError {
    /// The seat count is outside `game.yaml` `players`, or more than `ai.yaml` has opponents.
    #[error("players must be between {min} and {max}, got {got}")]
    Players {
        /// Fewest seats.
        min: u8,
        /// Most seats.
        max: u8,
        /// Asked for.
        got: u8,
    },
}

/// Counts bidding rounds and turns the auction house's state changes into [`Bid`] events.
#[derive(Debug, Clone, Default)]
pub struct AuctionWatch {
    last: Option<SimAuction>,
    outcomes_seen: usize,
    round: u32,
}

impl AuctionWatch {
    /// A watch that has seen everything `world` has done so far.
    pub fn new(world: &World) -> Self {
        AuctionWatch {
            last: world.auctions().current().cloned(),
            outcomes_seen: world.auctions().history().len(),
            round: 0,
        }
    }

    /// The bidding round of the live auction (0 in the countdown).
    pub fn round(&self) -> u32 {
        self.round
    }

    /// Compares the auction house with what it was after the last call and pushes what
    /// happened, oldest first: the passes and raises on the auction that was live, how it
    /// closed, then the next one opening.
    pub fn observe(&mut self, world: &World, out: &mut Vec<Bid>) {
        let tick = world.clock().tick();
        let house = world.auctions();
        let current = house.current();
        let history = house.history();
        let closed = &history[self.outcomes_seen.min(history.len())..];
        let player = |p: PlayerId| Some(protocol::player_id(p.0));
        let block_of = |item| view::block_key(world, view::item_block(world, item));
        let still_live =
            |prev: &SimAuction| closed.is_empty() && current.is_some_and(|c| c.item == prev.item);
        if let Some(prev) = self.last.as_ref() {
            if let Some(now) = current.filter(|_| still_live(prev)) {
                let block = block_of(now.item);
                if now.high_bid != prev.high_bid || now.leader != prev.leader {
                    out.push(Bid {
                        tick,
                        block,
                        player: now.leader.and_then(player),
                        amount: now.high_bid.pence(),
                        kind: BidKind::Raise,
                    });
                } else if now.passes_in_a_row > prev.passes_in_a_row {
                    let who = match prev.phase {
                        AuctionPhase::Bidding { turn, .. } => player(turn),
                        AuctionPhase::Countdown { .. } => None,
                    };
                    out.push(Bid {
                        tick,
                        block,
                        player: who,
                        amount: now.high_bid.pence(),
                        kind: BidKind::Pass,
                    });
                }
                self.round = next_round(self.round, prev, now);
            }
        }
        for outcome in closed {
            let (item, who, amount, kind) = match *outcome {
                AuctionOutcome::Sold {
                    item,
                    winner,
                    price,
                    ..
                } => (item, player(winner), price, BidKind::Won),
                AuctionOutcome::Defaulted {
                    item,
                    winner,
                    price,
                    ..
                } => (item, player(winner), price, BidKind::Unsold),
                AuctionOutcome::Unsold { item, .. } => (
                    item,
                    None,
                    self.last.as_ref().map_or(Money::ZERO, |a| a.high_bid),
                    BidKind::Unsold,
                ),
            };
            out.push(Bid {
                tick,
                block: block_of(item),
                player: who,
                amount: amount.pence(),
                kind,
            });
        }
        let opened = match (self.last.as_ref(), current) {
            (_, None) => false,
            (None, Some(_)) => true,
            (Some(prev), Some(_)) => !still_live(prev),
        };
        if let Some(now) = current.filter(|_| opened) {
            self.round = match now.phase {
                AuctionPhase::Countdown { .. } => 0,
                AuctionPhase::Bidding { .. } => 1,
            };
            out.push(Bid {
                tick,
                block: block_of(now.item),
                player: now.opener.and_then(player),
                amount: now.high_bid.pence(),
                kind: BidKind::Open,
            });
        }
        if current.is_none() {
            self.round = 0;
        }
        self.outcomes_seen = history.len();
        self.last = current.cloned();
    }
}

/// Bidding starts round 1; each time the turn comes back round the table (to a seat at or before
/// the last one) a new round starts.
fn next_round(round: u32, prev: &SimAuction, now: &SimAuction) -> u32 {
    match (prev.phase, now.phase) {
        (AuctionPhase::Countdown { .. }, AuctionPhase::Bidding { .. }) => 1,
        (AuctionPhase::Bidding { turn: a, .. }, AuctionPhase::Bidding { turn: b, .. })
            if b.0 <= a.0 && a != b =>
        {
            round + 1
        }
        _ => round,
    }
}

/// Each business's takings over the last completed game day.
#[derive(Debug, Clone, Default)]
struct TakingsBook {
    last_day: DayTakings,
}

impl TakingsBook {
    /// Call before each tick: if the tick about to run is midnight (which resets the day's
    /// takings), keep today's first.
    fn before_tick(&mut self, world: &World) {
        let per_day = u64::from(world.clock().rules().ticks_per_day()).max(1);
        if (world.clock().tick() + 1) % per_day == 0 {
            self.last_day = world
                .businesses()
                .by_id
                .iter()
                .map(|(id, b)| (*id, b.takings_today))
                .collect();
        }
    }
}

/// One inspector game and what the client has been sent of it.
#[derive(Debug)]
pub struct Session {
    world: World,
    speed: u8,
    max_walkers: u32,
    recent_cap: usize,
    watch: AuctionWatch,
    takings: TakingsBook,
    recent: VecDeque<Bid>,
    pending: Vec<Bid>,
    sent_lots: Vec<Lot>,
    sent_players: Vec<Player>,
    over_sent: bool,
}

impl Session {
    /// A new game of `players` seats, every one played by an AI opponent from `ai.yaml`
    /// (seated from the last seat down, as `World::new` does), the viewer a spectator.
    pub fn new(
        config: Config,
        seed: u64,
        players: u8,
        server: &ServerConfig,
    ) -> Result<Session, SessionError> {
        let opponents = u8::try_from(config.ai.opponents.len()).unwrap_or(u8::MAX);
        let min = u8::try_from(config.game.players.min).unwrap_or(u8::MAX);
        let max = u8::try_from(config.game.players.max)
            .unwrap_or(u8::MAX)
            .min(opponents);
        if players < min || players > max {
            return Err(SessionError::Players {
                min,
                max,
                got: players,
            });
        }
        // Every seat an AI: the roster from the last seat down, as sim-cli seats a game.
        let ai: Vec<(PlayerId, String)> = config
            .ai
            .opponents
            .iter()
            .zip((0..players).rev())
            .map(|(personality, p)| (PlayerId(p), personality.clone()))
            .collect();
        let world = World::seated(config, seed, players, &ai);
        Ok(Session::from_world(world, server))
    }

    /// Watches an already-built world (a test or tool's).
    pub fn from_world(world: World, server: &ServerConfig) -> Session {
        Session {
            watch: AuctionWatch::new(&world),
            world,
            speed: server.start_speed,
            max_walkers: server.max_walkers,
            recent_cap: usize::try_from(server.recent_bids).unwrap_or(usize::MAX),
            takings: TakingsBook::default(),
            recent: VecDeque::new(),
            pending: Vec::new(),
            sent_lots: Vec::new(),
            sent_players: Vec::new(),
            over_sent: false,
        }
    }

    /// The world, read-only.
    pub fn world(&self) -> &World {
        &self.world
    }

    /// The speed step the client last set.
    pub fn speed(&self) -> u8 {
        self.speed
    }

    /// Sets the speed step the clock reports.
    pub fn set_speed(&mut self, speed: u8) {
        self.speed = speed;
    }

    /// True once the final whistle has gone.
    pub fn is_over(&self) -> bool {
        self.world.clock().is_over()
    }

    /// Runs one tick (nothing once the game is over) and records what the auction house did.
    pub fn step(&mut self) -> ClockEvents {
        if self.is_over() {
            return ClockEvents::default();
        }
        self.takings.before_tick(&self.world);
        let events = self.world.tick();
        let before = self.pending.len();
        self.watch.observe(&self.world, &mut self.pending);
        for bid in &self.pending[before..] {
            if self.recent_cap > 0 {
                if self.recent.len() >= self.recent_cap {
                    self.recent.pop_front();
                }
                self.recent.push_back(bid.clone());
            }
        }
        events
    }

    /// The whole visible game. It becomes what the next delta is compared against.
    pub fn snapshot(&mut self) -> Snapshot {
        let lots = view::lots(&self.world, &self.takings.last_day);
        let players = view::players(&self.world);
        self.pending.clear();
        self.sent_lots.clone_from(&lots);
        self.sent_players.clone_from(&players);
        Snapshot {
            tick: self.world.clock().tick(),
            clock: view::clock(&self.world, self.speed),
            board: view::board(&self.world),
            players,
            lots,
            auction: view::auction(&self.world, self.watch.round()),
            bids: self.recent.iter().cloned().collect(),
            walkers: view::walkers(&self.world, self.max_walkers),
        }
    }

    /// What changed since the last snapshot or delta: changed lots and players, new bids, and
    /// the live auction and walkers in full.
    pub fn delta(&mut self) -> Delta {
        let lots = view::lots(&self.world, &self.takings.last_day);
        let players = view::players(&self.world);
        let changed_lots = changed(&self.sent_lots, &lots);
        let changed_players = changed(&self.sent_players, &players);
        self.sent_lots = lots;
        self.sent_players = players;
        Delta {
            tick: self.world.clock().tick(),
            clock: view::clock(&self.world, self.speed),
            players: changed_players,
            lots: changed_lots,
            auction: view::auction(&self.world, self.watch.round()),
            bids: std::mem::take(&mut self.pending),
            walkers: view::walkers(&self.world, self.max_walkers),
        }
    }

    /// The final result, once, after the final whistle.
    pub fn game_over(&mut self) -> Option<GameOver> {
        if !self.is_over() || self.over_sent {
            return None;
        }
        self.over_sent = true;
        let players = view::players(&self.world);
        Some(GameOver {
            winner: view::winner(&players),
            players,
        })
    }
}

/// The items of `now` that differ from `before`. Both are in the same fixed order (board order
/// for lots, seat order for players), so they are compared position by position; if the
/// lengths differ everything is sent.
fn changed<T: Clone + PartialEq>(before: &[T], now: &[T]) -> Vec<T> {
    if before.len() != now.len() {
        return now.to_vec();
    }
    now.iter()
        .zip(before)
        .filter(|(n, b)| n != b)
        .map(|(n, _)| n.clone())
        .collect()
}
