//! Auctions: the single global queue, countdown, round-the-table bidding, passes, the turn
//! timer, the bid ceiling (§5.1; R5, R7, R8, R9, R12, R23, R33).
//!
//! Owner boundary: the auction state machine. It asks [`crate::economy`] for each bidder's
//! ceiling and hands the winner and price back; [`crate::land`] records the lease.
//!
//! Status: types only (M0). The state machine lands in M1.

use serde::{Deserialize, Serialize};

use crate::businesses::BusinessId;
use crate::economy::PlayerId;
use crate::land::BlockId;
use crate::money::Money;
use crate::utilities::UtilityKind;

/// What is being sold. Land auctions sell land only (R5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AuctionItem {
    /// A block's lease (R9, R33).
    Land(BlockId),
    /// A business, separately from its land (R5).
    Business(BusinessId),
    /// A utility block (R12).
    Utility(UtilityKind),
}

/// Where an auction is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AuctionPhase {
    /// Opening bid placed; bidding starts at `starts_at_tick`.
    Countdown {
        /// Tick bidding opens.
        starts_at_tick: u64,
    },
    /// Bidding round the table.
    Bidding {
        /// Whose turn.
        turn: PlayerId,
        /// Tick the turn times out (a pass).
        turn_ends_at_tick: u64,
    },
}

/// One auction.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Auction {
    /// What is sold.
    pub item: AuctionItem,
    /// Who opened it.
    pub opener: PlayerId,
    /// Highest bidder so far.
    pub leader: PlayerId,
    /// Highest bid so far.
    pub high_bid: Money,
    /// Phase.
    pub phase: AuctionPhase,
}

/// The game's auction house: at most one auction scheduled or running at a time.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct AuctionHouse {
    /// The current auction, if any.
    pub current: Option<Auction>,
}

impl AuctionHouse {
    /// True while an auction is scheduled or running; no other may open (§5.1).
    pub fn is_busy(&self) -> bool {
        self.current.is_some()
    }
}
