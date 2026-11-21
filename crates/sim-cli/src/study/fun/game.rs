//! What one game contributes to the fun score (R79), from its midnight boards: each seat's net
//! worth and bankruptcy at every midnight, as a recording's checkpoints hold them.
//!
//! - **Leader** at a midnight: the solvent seat(s) with the highest net worth.
//! - **Lead changes**: walking the midnights in order, the number of times a single leader
//!   differs from the last single leader seen. A shared lead (a tie at the top) or a midnight
//!   with no solvent seat keeps the previous leader; the first single leader is not a change.
//! - **Halfway midnight**: day `⌊D / 2⌋` of the `D` midnights played (day 12 of the 24 of the
//!   full 1930–2050 game, the 1990 midnight). None if `D < 2`.
//! - **Comeback**: a game with an outright winner whose seat was not among the leaders at the
//!   halfway midnight. Undecided for a game with no outright winner or no halfway midnight.
//! - **Winning margin**: `(winner − runner-up) / winner` in final net worth, where the
//!   runner-up is the best of the other seats (solvent ahead of bankrupt, then net worth),
//!   clamped to `[0, 1]`. None without an outright winner or when the winner's net worth is not
//!   above 0.
//! - **Holdings** of each seat at the end of the game (see [`Holdings`]).
//!
//! [`FunTicker`] collects the boards while a game plays; it only reads the world, so the game
//! is unchanged (same state hash).

use sim_core::World;
use sim_core::businesses::Hours;
use sim_core::clock::ClockEvents;
use sim_core::config::businesses::Channel;
use sim_core::economy::{AccountId, PlayerId};
use sim_core::land::BlockKind;
use sim_core::world::PlayerStatus;

use super::score::Playstyle;
use super::{PPM, div_round};
use crate::game::{Ticker, player_net_worth};

/// Each seat's `(net worth in pence, bankrupt)` at one midnight, in seat order.
pub type Board = Vec<(i64, bool)>;

/// One seat's holdings at the end of a game.
///
/// - `flats`: apartment buildings it owns (businesses with no trading hours);
/// - `shops`: walk-in businesses trading by day or night;
/// - `land`: buildable blocks it leases with none of its buildings on them;
/// - `hotels`: hotel buildings;
/// - `other`: online and infrastructure businesses, and leased utility blocks;
/// - `leases` and `businesses`: the two counts a recording's checkpoint holds (every block it
///   leases, every business it owns).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Holdings {
    /// Apartment buildings.
    pub flats: u32,
    /// Walk-in shops (day or night trade).
    pub shops: u32,
    /// Bare leased buildable blocks.
    pub land: u32,
    /// Hotels.
    pub hotels: u32,
    /// Online and infrastructure businesses, leased utility blocks.
    pub other: u32,
    /// Blocks leased (a checkpoint's `leases`).
    pub leases: u32,
    /// Businesses owned (a checkpoint's `businesses`).
    pub businesses: u32,
}

impl Holdings {
    /// The counts a playstyle is measured on: `[flats, shops, land, hotels, other]` or
    /// `[leases, businesses]`.
    pub fn mix(&self, basis: Playstyle) -> Vec<u64> {
        match basis {
            Playstyle::Holdings => [self.flats, self.shops, self.land, self.hotels, self.other]
                .iter()
                .map(|v| u64::from(*v))
                .collect(),
            Playstyle::Checkpoint => vec![u64::from(self.leases), u64::from(self.businesses)],
        }
    }
}

/// `player`'s holdings in `world` now.
pub fn holdings_of(world: &World, player: PlayerId) -> Holdings {
    let me = AccountId::Player(player);
    let types = &world.config().businesses.types;
    let mut h = Holdings::default();
    let mut built_on = std::collections::BTreeSet::new();
    for (_, b) in world.businesses().owned_by(me) {
        h.businesses += 1;
        built_on.insert(b.block);
        let online = types
            .get(&b.type_id)
            .is_some_and(|t| t.channel != Channel::Shop);
        match b.hours {
            Hours::None => h.flats += 1,
            Hours::Hotel => h.hotels += 1,
            Hours::Day | Hours::Night if online => h.other += 1,
            Hours::Day | Hours::Night => h.shops += 1,
        }
    }
    for (i, block) in world.map().blocks.iter().enumerate() {
        if block.lease.as_ref().is_none_or(|l| l.holder != me) {
            continue;
        }
        h.leases += 1;
        let id = sim_core::land::BlockId(u32::try_from(i).unwrap_or(u32::MAX));
        match block.kind {
            BlockKind::Buildable if !built_on.contains(&id) => h.land += 1,
            BlockKind::Buildable => {}
            BlockKind::Utility | BlockKind::Special => h.other += 1,
        }
    }
    h
}

/// Every seat's net worth and bankruptcy in `world` now.
pub fn board(world: &World) -> Board {
    world
        .players()
        .iter()
        .map(|(p, s)| {
            (
                player_net_worth(world, *p).pence(),
                *s == PlayerStatus::Bankrupt,
            )
        })
        .collect()
}

/// The solvent seats with the highest net worth (several on a tie; none if nobody is solvent).
pub fn leaders(board: &[(i64, bool)]) -> Vec<u8> {
    let Some(top) = board.iter().filter(|(_, b)| !b).map(|(w, _)| *w).max() else {
        return Vec::new();
    };
    board
        .iter()
        .enumerate()
        .filter(|(_, (w, b))| !b && *w == top)
        .filter_map(|(s, _)| u8::try_from(s).ok())
        .collect()
}

/// Lead changes over the midnights, in order (see the module docs).
pub fn lead_changes(boards: &[Board]) -> u32 {
    // red: not yet implemented
    let _ = boards;
    0
}

/// The winning margin in parts per million (see the module docs).
pub fn winning_margin_ppm(last: &[(i64, bool)], winner: Option<u8>) -> Option<i64> {
    // red: not yet implemented
    let _ = (last, winner);
    None
}

/// One game's fun data.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FunGame {
    /// Midnights measured.
    pub midnights: u32,
    /// Lead changes.
    pub lead_changes: u32,
    /// The halfway midnight's day (0: none).
    pub halfway_day: u32,
    /// The seats leading at the halfway midnight (several on a tie).
    pub halfway_leaders: Vec<u8>,
    /// Whether the winner came back from behind at halfway; `None` if undecided.
    pub comeback: Option<bool>,
    /// Winning margin, parts per million; `None` if undefined.
    pub margin_ppm: Option<i64>,
    /// Each seat's holdings at the end.
    pub holdings: Vec<Holdings>,
    /// True if `holdings` came from the world (every category known); false if they came from
    /// a checkpoint (only `leases` and `businesses`).
    pub full_holdings: bool,
}

/// A game's fun data from its midnight boards (in order; the last is the final board), its
/// outright winner's seat, and each seat's holdings at the end.
pub fn fun_game(
    boards: &[Board],
    winner: Option<u8>,
    holdings: Vec<Holdings>,
    full_holdings: bool,
) -> FunGame {
    let midnights = u32::try_from(boards.len()).unwrap_or(u32::MAX);
    let halfway_day = if midnights >= 2 { midnights / 2 } else { 0 };
    let halfway_leaders = halfway_day
        .checked_sub(1)
        .and_then(|i| boards.get(i as usize))
        .map(|b| leaders(b))
        .unwrap_or_default();
    // red: not yet implemented
    let comeback = None;
    FunGame {
        midnights,
        lead_changes: lead_changes(boards),
        halfway_day,
        halfway_leaders,
        comeback,
        margin_ppm: boards.last().and_then(|b| winning_margin_ppm(b, winner)),
        holdings,
        full_holdings,
    }
}

/// A [`Ticker`] that runs `inner` and keeps the board of every midnight. Read-only.
#[derive(Debug, Default)]
pub struct FunTicker<T> {
    inner: T,
    boards: Vec<Board>,
}

impl<T> FunTicker<T> {
    /// Wraps `inner`.
    pub fn new(inner: T) -> Self {
        FunTicker {
            inner,
            boards: Vec::new(),
        }
    }

    /// The boards so far, one per midnight.
    pub fn boards(&self) -> &[Board] {
        &self.boards
    }

    /// The boards and the inner ticker.
    pub fn into_parts(self) -> (Vec<Board>, T) {
        (self.boards, self.inner)
    }
}

impl<T: Ticker> Ticker for FunTicker<T> {
    fn tick(&mut self, world: &mut World, external: usize) -> ClockEvents {
        let fired = self.inner.tick(world, external);
        if fired.midnight {
            self.boards.push(board(world));
        }
        fired
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(v: &[i64]) -> Board {
        v.iter().map(|w| (*w, false)).collect()
    }

    #[test]
    fn ties_keep_the_previous_leader() {
        let boards = vec![b(&[5, 1]), b(&[3, 3]), b(&[5, 1]), b(&[1, 5])];
        assert_eq!(lead_changes(&boards), 1);
    }

    #[test]
    fn bankrupt_seats_never_lead() {
        let boards = vec![vec![(9, true), (1, false)], vec![(9, true), (2, false)]];
        assert_eq!(lead_changes(&boards), 0);
        assert_eq!(leaders(&boards[0]), vec![1]);
    }

    #[test]
    fn margins_use_the_best_other_seat_and_clamp() {
        assert_eq!(
            winning_margin_ppm(&b(&[100, 75, 10]), Some(0)),
            Some(250_000)
        );
        assert_eq!(
            winning_margin_ppm(&[(100, false), (-50, true)], Some(0)),
            Some(PPM)
        );
        assert_eq!(winning_margin_ppm(&b(&[0, -5]), Some(0)), None);
        assert_eq!(winning_margin_ppm(&b(&[5, 5]), None), None);
    }
}
