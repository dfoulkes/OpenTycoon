//! Auctions: the single global queue, the countdown, round-the-table bidding, passes, the turn
//! timer and the bid ceiling (§2.3, §5.1; R5, R8, R9, R23, R33).
//!
//! Owner boundary: the auction state machine in [`AuctionHouse`]. It never touches money or
//! leases: the world tells it each bidder's ceiling (`balance + credit limit`, R7, from
//! [`crate::economy`]) and the minimum bid for the item (R23), and it hands back an
//! [`AuctionOutcome`] that the world applies (the winner pays the seller at once; land goes to
//! [`crate::land`], businesses to [`crate::businesses`]).
//!
//! The rules it enforces:
//!
//! - **One at a time.** Only one auction may be scheduled or running across the game. A
//!   player's opening bid or offer while one is on is refused; sales the game itself starts
//!   (lease expiry, R9) wait in one first-in, first-out queue.
//! - **Countdown.** Bidding starts `countdown_real_seconds` (120) real seconds after the
//!   auction opens, for every item and every seller: no shortcuts (R5).
//! - **Round the table.** Turns go left to right in seat order, starting with the player after
//!   the opener (or after the seller, or seat 0 when the city sells). The highest bidder is
//!   skipped; a seller never bids on their own item.
//! - **Raises** are one of the configured increments (£50 / £100 / £500 / £1,000) on top of the
//!   current bid, up to the bidder's ceiling (R7).
//! - **Passing** counts for that round only: the player gets their turn again next round (R8).
//!   Running out the `turn_real_seconds` (10) timer is a pass.
//! - **Closing.** Once everyone who could beat the high bid has passed in a row (a full round
//!   with no new bid), the auction closes and the highest bidder wins (R8). With no bid at all
//!   the item goes unsold and stays with its seller.

mod bidding;
mod timing;

use serde::{Deserialize, Serialize};

use crate::businesses::BusinessId;
use crate::economy::{AccountId, PlayerId};
use crate::land::BlockId;
use crate::money::Money;
use crate::utilities::UtilityKind;

pub use timing::{AuctionRules, real_seconds_to_ticks};

/// What is being sold. Land auctions sell land only; businesses are auctioned on their own
/// (R5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AuctionItem {
    /// A block's lease (R9, R33).
    Land(BlockId),
    /// A business, separately from its land (R5).
    Business(BusinessId),
    /// A utility block (R12; post-v1, refused for now).
    Utility(UtilityKind),
}

/// Where an auction is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AuctionPhase {
    /// Opened; bidding starts at `starts_at_tick`.
    Countdown {
        /// Tick bidding opens.
        starts_at_tick: u64,
    },
    /// Bidding round the table.
    Bidding {
        /// Whose turn.
        turn: PlayerId,
        /// Tick the turn runs out (a pass).
        turn_ends_at_tick: u64,
    },
}

/// One auction.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Auction {
    /// What is sold.
    pub item: AuctionItem,
    /// Who is paid: the city, or the player selling their lease or business.
    pub seller: AccountId,
    /// Who opened it with an opening bid, if a player did. Turns start after them.
    pub opener: Option<PlayerId>,
    /// Highest bidder so far, if anyone has bid.
    pub leader: Option<PlayerId>,
    /// Highest bid so far, or the reserve before anyone has bid.
    pub high_bid: Money,
    /// Phase.
    pub phase: AuctionPhase,
    /// Turns in a row that passed without a new bid (reset by every bid).
    pub passes_in_a_row: u32,
}

/// How an auction ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AuctionOutcome {
    /// The highest bidder won and pays the seller `price` at once.
    Sold {
        /// What was sold.
        item: AuctionItem,
        /// Who is paid.
        seller: AccountId,
        /// Highest bidder.
        winner: PlayerId,
        /// Winning bid.
        price: Money,
    },
    /// The winner could not pay at the close (bankrupt, or past their ceiling after the 06:00
    /// bills); nothing changes hands and the item stays with its seller.
    Defaulted {
        /// What was offered.
        item: AuctionItem,
        /// Who offered it.
        seller: AccountId,
        /// The bidder who could not pay.
        winner: PlayerId,
        /// Their winning bid.
        price: Money,
    },
    /// A full round went by with no bid at all; the item stays with its seller.
    Unsold {
        /// What was offered.
        item: AuctionItem,
        /// Who offered it.
        seller: AccountId,
    },
}

/// Why an auction command was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AuctionError {
    /// Another auction is scheduled or running (§5.1).
    Busy,
    /// No auction is running.
    NoAuction,
    /// Still counting down; bidding has not started.
    InCountdown,
    /// It is someone else's turn.
    NotYourTurn,
    /// The raise is not one of the configured increments.
    BadIncrement,
    /// The bid is below the minimum (the district's land base price for land, R23).
    BelowMinimum {
        /// The minimum.
        minimum: Money,
    },
    /// The bid is above the bidder's ceiling, `balance + credit limit` (R7).
    OverCeiling {
        /// The ceiling.
        ceiling: Money,
    },
    /// Not open for bids: someone else holds it, and only its holder can put it up (R33).
    NotForSale,
    /// Only the holder or owner can offer it.
    NotYours,
    /// No such block or business, or a kind of item not auctioned in v1.
    Unknown,
}

/// A refused command, kept so clients (and tests) can say why.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Refusal {
    /// Who sent it.
    pub player: PlayerId,
    /// Why it was refused.
    pub error: AuctionError,
}

/// A request to open an auction, already checked against the map or businesses by the world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Opening {
    /// What is sold.
    pub item: AuctionItem,
    /// Who is paid.
    pub seller: AccountId,
    /// The player placing an opening bid, if any, and the bid.
    pub opening_bid: Option<(PlayerId, Money)>,
    /// The least the opening bid may be (R23), or the reserve bids raise from when there is
    /// no opening bid.
    pub minimum: Money,
}

/// A sale the game started itself, waiting for the house to be free (R9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct QueuedSale {
    /// What is sold.
    pub item: AuctionItem,
    /// Who is paid.
    pub seller: AccountId,
    /// The reserve bids raise from.
    pub reserve: Money,
}

/// The game's auction house: at most one auction scheduled or running at a time, plus the
/// queue of sales waiting their turn.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct AuctionHouse {
    rules: AuctionRules,
    current: Option<Auction>,
    queue: Vec<QueuedSale>,
    history: Vec<AuctionOutcome>,
    last_refusal: Option<Refusal>,
}

impl AuctionHouse {
    /// An empty house running on `rules`.
    pub fn new(rules: AuctionRules) -> Self {
        AuctionHouse {
            rules,
            ..AuctionHouse::default()
        }
    }

    /// The rules in force.
    pub fn rules(&self) -> &AuctionRules {
        &self.rules
    }

    /// The auction scheduled or running, if any.
    pub fn current(&self) -> Option<&Auction> {
        self.current.as_ref()
    }

    /// True while an auction is scheduled or running; no other may open (§5.1).
    pub fn is_busy(&self) -> bool {
        self.current.is_some()
    }

    /// Sales waiting for the house, oldest first.
    pub fn queue(&self) -> &[QueuedSale] {
        &self.queue
    }

    /// Every closed auction, oldest first.
    pub fn history(&self) -> &[AuctionOutcome] {
        &self.history
    }

    /// The last refused auction command, cleared by the next accepted one.
    pub fn last_refusal(&self) -> Option<Refusal> {
        self.last_refusal
    }

    /// Records the result of a player's auction command.
    pub fn record(&mut self, player: PlayerId, result: Result<(), AuctionError>) {
        self.last_refusal = result.err().map(|error| Refusal { player, error });
    }

    /// Marks the last closed sale as defaulted: the world found the winner could not pay.
    pub fn mark_defaulted(&mut self) {
        if let Some(last) = self.history.last_mut()
            && let AuctionOutcome::Sold {
                item,
                seller,
                winner,
                price,
            } = *last
        {
            *last = AuctionOutcome::Defaulted {
                item,
                seller,
                winner,
                price,
            };
        }
    }

    /// True if `item` is up for auction now or waiting in the queue.
    pub fn is_listed(&self, item: AuctionItem) -> bool {
        self.current.as_ref().is_some_and(|a| a.item == item)
            || self.queue.iter().any(|s| s.item == item)
    }

    /// Opens an auction at tick `now`; bidding starts after the countdown. Refused while
    /// another is scheduled or running, below the minimum, or over the opener's `ceiling`.
    /// A queued sale of the same item is dropped: this auction replaces it.
    pub fn open(&mut self, opening: Opening, ceiling: Money, now: u64) -> Result<(), AuctionError> {
        if self.is_busy() {
            return Err(AuctionError::Busy);
        }
        let (opener, leader, high_bid) = match opening.opening_bid {
            Some((player, bid)) => {
                if bid < opening.minimum {
                    return Err(AuctionError::BelowMinimum {
                        minimum: opening.minimum,
                    });
                }
                if bid > ceiling {
                    return Err(AuctionError::OverCeiling { ceiling });
                }
                (Some(player), Some(player), bid)
            }
            None => (None, None, opening.minimum),
        };
        self.queue.retain(|s| s.item != opening.item);
        self.current = Some(Auction {
            item: opening.item,
            seller: opening.seller,
            opener,
            leader,
            high_bid,
            phase: AuctionPhase::Countdown {
                starts_at_tick: now + self.rules.countdown_ticks,
            },
            passes_in_a_row: 0,
        });
        Ok(())
    }

    /// Queues a sale the game starts itself (lease expiry, R9). It opens, with no opening
    /// bid, as soon as the house is free.
    pub fn enqueue(&mut self, sale: QueuedSale) {
        if !self.is_listed(sale.item) {
            self.queue.push(sale);
        }
    }

    /// `player` raises the high bid by `by` on their turn. `ceiling` is their
    /// `balance + credit limit` (R7).
    pub fn raise(
        &mut self,
        player: PlayerId,
        by: Money,
        ceiling: Money,
        now: u64,
        bidders: &[PlayerId],
    ) -> Result<(), AuctionError> {
        let turn_ticks = self.rules.turn_ticks;
        let valid_increment = self.rules.increments.contains(&by);
        let auction = self.my_turn(player)?;
        if !valid_increment {
            return Err(AuctionError::BadIncrement);
        }
        let bid = auction.high_bid + by;
        if bid > ceiling {
            return Err(AuctionError::OverCeiling { ceiling });
        }
        auction.high_bid = bid;
        auction.leader = Some(player);
        auction.passes_in_a_row = 0;
        bidding::next_turn(auction, player, bidders, now, turn_ticks);
        Ok(())
    }

    /// `player` passes on their turn. It counts for this round only (R8).
    pub fn pass(
        &mut self,
        player: PlayerId,
        now: u64,
        bidders: &[PlayerId],
    ) -> Result<(), AuctionError> {
        let turn_ticks = self.rules.turn_ticks;
        let auction = self.my_turn(player)?;
        auction.passes_in_a_row += 1;
        bidding::next_turn(auction, player, bidders, now, turn_ticks);
        Ok(())
    }

    fn my_turn(&mut self, player: PlayerId) -> Result<&mut Auction, AuctionError> {
        let auction = self.current.as_mut().ok_or(AuctionError::NoAuction)?;
        match auction.phase {
            AuctionPhase::Countdown { .. } => Err(AuctionError::InCountdown),
            AuctionPhase::Bidding { turn, .. } if turn != player => Err(AuctionError::NotYourTurn),
            AuctionPhase::Bidding { .. } => Ok(auction),
        }
    }

    /// The auctions phase (tick order step 6): ends the countdown, times out a turn (a pass),
    /// closes the auction after a full round with no new bid, and opens the next queued sale.
    /// `bidders` are the players still in the game, in seat order.
    pub fn advance(&mut self, now: u64, bidders: &[PlayerId]) -> Option<AuctionOutcome> {
        let turn_ticks = self.rules.turn_ticks;
        let mut outcome = None;
        if let Some(auction) = self.current.as_mut() {
            match auction.phase {
                AuctionPhase::Countdown { starts_at_tick } if now >= starts_at_tick => {
                    bidding::start(auction, bidders, now, turn_ticks);
                }
                AuctionPhase::Bidding {
                    turn,
                    turn_ends_at_tick,
                } if now >= turn_ends_at_tick => {
                    auction.passes_in_a_row += 1;
                    bidding::next_turn(auction, turn, bidders, now, turn_ticks);
                }
                _ => {}
            }
            if bidding::round_is_over(auction, bidders) {
                outcome = Some(bidding::close(auction));
            }
        }
        if let Some(done) = outcome {
            self.current = None;
            self.history.push(done);
        }
        if self.current.is_none() && !self.queue.is_empty() {
            let sale = self.queue.remove(0);
            let opening = Opening {
                item: sale.item,
                seller: sale.seller,
                opening_bid: None,
                minimum: sale.reserve,
            };
            // The house is free and there is no opening bid, so this cannot be refused.
            let _ = self.open(opening, Money::ZERO, now);
        }
        outcome
    }
}

#[cfg(test)]
mod tests;
