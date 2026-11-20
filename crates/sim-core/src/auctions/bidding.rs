//! Round-the-table turns (§5.1, R8): who bids next, when a round is over, who won.

use super::{Auction, AuctionOutcome, AuctionPhase};
use crate::economy::{AccountId, PlayerId};

/// Players who may bid on this auction: everyone still in the game except the seller.
fn eligible<'a>(
    auction: &'a Auction,
    bidders: &'a [PlayerId],
) -> impl Iterator<Item = PlayerId> + 'a {
    bidders
        .iter()
        .copied()
        .filter(move |p| AccountId::Player(*p) != auction.seller)
}

/// Players who could still beat the high bid: the eligible bidders bar the leader.
fn challengers<'a>(
    auction: &'a Auction,
    bidders: &'a [PlayerId],
) -> impl Iterator<Item = PlayerId> + 'a {
    eligible(auction, bidders).filter(move |p| Some(*p) != auction.leader)
}

/// The next challenger to the left of `after` (higher seat, wrapping round), or the first in
/// seat order when there is no one to start after.
fn next_after(
    auction: &Auction,
    after: Option<PlayerId>,
    bidders: &[PlayerId],
) -> Option<PlayerId> {
    let mut seats: Vec<PlayerId> = challengers(auction, bidders).collect();
    seats.sort_unstable();
    let first = seats.first().copied();
    match after {
        Some(after) => seats.into_iter().find(|p| *p > after).or(first),
        None => first,
    }
}

/// Opens bidding: the first turn goes to the player after the opener, else after the selling
/// player, else to the first seat (the city is selling).
pub(super) fn start(auction: &mut Auction, bidders: &[PlayerId], now: u64, turn_ticks: u64) {
    let after = auction.opener.or(match auction.seller {
        AccountId::Player(p) => Some(p),
        AccountId::City => None,
    });
    give_turn(auction, after, bidders, now, turn_ticks);
}

/// Moves the turn on from `after` to the next challenger, with a fresh turn timer.
pub(super) fn next_turn(
    auction: &mut Auction,
    after: PlayerId,
    bidders: &[PlayerId],
    now: u64,
    turn_ticks: u64,
) {
    give_turn(auction, Some(after), bidders, now, turn_ticks);
}

fn give_turn(
    auction: &mut Auction,
    after: Option<PlayerId>,
    bidders: &[PlayerId],
    now: u64,
    turn_ticks: u64,
) {
    // With no challenger left there is no turn to give; `round_is_over` closes the auction.
    let turn = next_after(auction, after, bidders)
        .or(auction.leader)
        .unwrap_or_default();
    auction.phase = AuctionPhase::Bidding {
        turn,
        turn_ends_at_tick: now + turn_ticks,
    };
}

/// True once every challenger has passed in a row since the last bid: a full round with no
/// new bid (R8). Never true during the countdown.
pub(super) fn round_is_over(auction: &Auction, bidders: &[PlayerId]) -> bool {
    if matches!(auction.phase, AuctionPhase::Countdown { .. }) {
        return false;
    }
    let needed = challengers(auction, bidders).count();
    u64::from(auction.passes_in_a_row) >= needed as u64
}

/// How the auction ended: the highest bidder wins at their bid, or nobody bid at all.
pub(super) fn close(auction: &Auction) -> AuctionOutcome {
    match auction.leader {
        Some(winner) => AuctionOutcome::Sold {
            item: auction.item,
            seller: auction.seller,
            winner,
            price: auction.high_bid,
        },
        None => AuctionOutcome::Unsold {
            item: auction.item,
            seller: auction.seller,
        },
    }
}
