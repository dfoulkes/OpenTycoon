use proptest::prelude::*;

use super::*;
use crate::clock::ClockRules;
use crate::config::game::ClockSettings;

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);
const P2: PlayerId = PlayerId(2);
const ALL: [PlayerId; 3] = [P0, P1, P2];
const BLOCK: AuctionItem = AuctionItem::Land(BlockId(0));
const RICH: Money = Money::from_pounds(1_000_000);

fn rules() -> AuctionRules {
    AuctionRules {
        countdown_ticks: 864,
        turn_ticks: 72,
        increments: [50, 100, 500, 1_000].map(Money::from_pounds).to_vec(),
    }
}

fn land_opened_by(player: PlayerId, bid: i64) -> AuctionHouse {
    let mut house = AuctionHouse::new(rules());
    house
        .open(
            Opening {
                item: BLOCK,
                seller: AccountId::City,
                opening_bid: Some((player, Money::from_pounds(bid))),
                minimum: Money::from_pounds(2_000),
            },
            RICH,
            0,
        )
        .unwrap();
    house
}

fn turn(house: &AuctionHouse) -> Option<PlayerId> {
    match house.current()?.phase {
        AuctionPhase::Bidding { turn, .. } => Some(turn),
        AuctionPhase::Countdown { .. } => None,
    }
}

#[test]
fn timers_convert_real_seconds_to_ticks_at_the_default_clock() {
    let settings: ClockSettings = serde_yaml_ng::from_str(
        r#"
real_minutes_per_day: 20
years_per_day: 5
start_year: 1930
end_year: 2050
tick_game_seconds: 10
trading_hours:
  day: { open: "07:00", close: "17:00" }
  night: { open: "19:00", close: "23:50" }
  hotel: { open: "23:30", close: "03:00" }
schedule: { arrivals: "05:30", settlement: "06:00" }
"#,
    )
    .unwrap();
    let clock = ClockRules::from_config(&settings);
    assert_eq!(real_seconds_to_ticks(120, &clock), 864);
    assert_eq!(real_seconds_to_ticks(10, &clock), 72);
    // 7.2 ticks a second: a part tick rounds up so a timer never runs short.
    assert_eq!(real_seconds_to_ticks(1, &clock), 8);
}

#[test]
fn bidding_starts_after_the_countdown_with_the_player_after_the_opener() {
    let mut house = land_opened_by(P1, 2_000);
    assert!(house.advance(863, &ALL).is_none());
    assert_eq!(turn(&house), None);
    house.advance(864, &ALL);
    assert_eq!(turn(&house), Some(P2));
}

#[test]
fn the_leader_is_skipped_and_turns_wrap_left_to_right() {
    let mut house = land_opened_by(P1, 2_000);
    house.advance(864, &ALL);
    house.pass(P2, 865, &ALL).unwrap();
    assert_eq!(
        turn(&house),
        Some(P0),
        "wraps past the last seat, skipping leader P1"
    );
    house
        .raise(P0, Money::from_pounds(50), RICH, 866, &ALL)
        .unwrap();
    assert_eq!(turn(&house), Some(P1));
}

#[test]
fn a_full_round_of_passes_closes_and_the_leader_wins() {
    let mut house = land_opened_by(P0, 2_000);
    house.advance(864, &ALL);
    house.pass(P1, 865, &ALL).unwrap();
    assert!(house.advance(865, &ALL).is_none());
    house.pass(P2, 866, &ALL).unwrap();
    assert_eq!(
        house.advance(866, &ALL),
        Some(AuctionOutcome::Sold {
            item: BLOCK,
            seller: AccountId::City,
            winner: P0,
            price: Money::from_pounds(2_000),
        })
    );
    assert!(!house.is_busy());
}

#[test]
fn a_pass_counts_for_one_round_only() {
    let mut house = land_opened_by(P0, 2_000);
    house.advance(864, &ALL);
    house.pass(P1, 865, &ALL).unwrap();
    house
        .raise(P2, Money::from_pounds(100), RICH, 866, &ALL)
        .unwrap();
    house.pass(P0, 867, &ALL).unwrap();
    // P1 passed last round and is back in.
    house
        .raise(P1, Money::from_pounds(50), RICH, 868, &ALL)
        .unwrap();
    assert_eq!(house.current().unwrap().leader, Some(P1));
}

#[test]
fn a_turn_that_runs_out_is_a_pass() {
    let mut house = land_opened_by(P0, 2_000);
    house.advance(864, &ALL);
    assert_eq!(turn(&house), Some(P1));
    house.advance(935, &ALL);
    assert_eq!(turn(&house), Some(P1), "71 ticks in: still P1's turn");
    house.advance(936, &ALL);
    assert_eq!(turn(&house), Some(P2));
    let outcome = house.advance(936 + 72, &ALL);
    assert!(matches!(
        outcome,
        Some(AuctionOutcome::Sold { winner: P0, .. })
    ));
}

#[test]
fn refusals_name_the_reason() {
    let mut house = land_opened_by(P0, 2_000);
    let busy = Opening {
        item: AuctionItem::Land(BlockId(1)),
        seller: AccountId::City,
        opening_bid: Some((P1, Money::from_pounds(5_000))),
        minimum: Money::ZERO,
    };
    assert_eq!(house.open(busy, RICH, 1), Err(AuctionError::Busy));
    assert_eq!(house.pass(P1, 1, &ALL), Err(AuctionError::InCountdown));
    house.advance(864, &ALL);
    assert_eq!(house.pass(P2, 865, &ALL), Err(AuctionError::NotYourTurn));
    assert_eq!(
        house.raise(P1, Money::from_pounds(75), RICH, 865, &ALL),
        Err(AuctionError::BadIncrement)
    );
    assert_eq!(
        house.raise(
            P1,
            Money::from_pounds(50),
            Money::from_pounds(2_049),
            865,
            &ALL
        ),
        Err(AuctionError::OverCeiling {
            ceiling: Money::from_pounds(2_049)
        })
    );
    assert_eq!(
        turn(&house),
        Some(P1),
        "a refused bid does not use up the turn"
    );
}

#[test]
fn an_opening_bid_must_meet_the_minimum_and_the_ceiling() {
    let mut house = AuctionHouse::new(rules());
    let opening = |bid| Opening {
        item: BLOCK,
        seller: AccountId::City,
        opening_bid: Some((P0, Money::from_pounds(bid))),
        minimum: Money::from_pounds(2_000),
    };
    assert_eq!(
        house.open(opening(1_999), RICH, 0),
        Err(AuctionError::BelowMinimum {
            minimum: Money::from_pounds(2_000)
        })
    );
    assert_eq!(
        house.open(opening(2_000), Money::from_pounds(1_000), 0),
        Err(AuctionError::OverCeiling {
            ceiling: Money::from_pounds(1_000)
        })
    );
    assert!(house.open(opening(2_000), RICH, 0).is_ok());
}

#[test]
fn a_seller_never_bids_and_no_bid_means_unsold() {
    let mut house = AuctionHouse::new(rules());
    house
        .open(
            Opening {
                item: BLOCK,
                seller: AccountId::Player(P1),
                opening_bid: None,
                minimum: Money::from_pounds(2_000),
            },
            Money::ZERO,
            0,
        )
        .unwrap();
    house.advance(864, &ALL);
    assert_eq!(turn(&house), Some(P2), "starts after the selling player");
    house.pass(P2, 865, &ALL).unwrap();
    assert_eq!(turn(&house), Some(P0), "skips the seller");
    house.pass(P0, 866, &ALL).unwrap();
    assert_eq!(
        house.advance(866, &ALL),
        Some(AuctionOutcome::Unsold {
            item: BLOCK,
            seller: AccountId::Player(P1)
        })
    );
}

#[test]
fn queued_sales_open_one_at_a_time_when_the_house_is_free() {
    let mut house = land_opened_by(P0, 2_000);
    let sale = |b| QueuedSale {
        item: AuctionItem::Land(BlockId(b)),
        seller: AccountId::City,
        reserve: Money::from_pounds(2_000),
    };
    house.enqueue(sale(1));
    house.enqueue(sale(2));
    house.enqueue(sale(1));
    assert_eq!(house.queue().len(), 2, "an item is listed once");
    house.advance(864, &ALL);
    house.pass(P1, 865, &ALL).unwrap();
    house.pass(P2, 866, &ALL).unwrap();
    house.advance(866, &ALL);
    assert_eq!(house.current().unwrap().item, AuctionItem::Land(BlockId(1)));
    assert_eq!(
        house.current().unwrap().phase,
        AuctionPhase::Countdown {
            starts_at_tick: 866 + 864
        }
    );
    assert_eq!(house.queue().len(), 1);
}

#[derive(Debug, Clone)]
enum Action {
    Open(u8, u32, i64),
    Raise(u8, i64, i64),
    Pass(u8),
    Wait(u64),
}

fn action() -> impl Strategy<Value = Action> {
    prop_oneof![
        (0u8..4, 0u32..3, 0i64..20_000).prop_map(|(p, b, bid)| Action::Open(p, b, bid)),
        (
            0u8..4,
            prop::sample::select(vec![50i64, 75, 100, 500, 1_000]),
            0i64..20_000
        )
            .prop_map(|(p, by, ceiling)| Action::Raise(p, by, ceiling)),
        (0u8..4).prop_map(Action::Pass),
        (1u64..200).prop_map(Action::Wait),
    ]
}

proptest! {
    /// One auction at a time, and no accepted bid ever exceeds its bidder's ceiling (§5.1, R7).
    #[test]
    fn one_auction_at_a_time_and_bids_never_exceed_the_ceiling(
        actions in prop::collection::vec(action(), 0..200)
    ) {
        let bidders = [P0, P1, P2, PlayerId(3)];
        let mut house = AuctionHouse::new(rules());
        let mut now = 0u64;
        let mut opened = 0usize;
        for a in actions {
            match a {
                Action::Open(p, block, bid) => {
                    let was_busy = house.is_busy();
                    let result = house.open(
                        Opening {
                            item: AuctionItem::Land(BlockId(block)),
                            seller: AccountId::City,
                            opening_bid: Some((PlayerId(p), Money::from_pounds(bid))),
                            minimum: Money::from_pounds(2_000),
                        },
                        Money::from_pounds(15_000),
                        now,
                    );
                    if was_busy {
                        prop_assert_eq!(result, Err(AuctionError::Busy));
                    }
                    if result.is_ok() {
                        opened += 1;
                        let a = house.current().unwrap();
                        prop_assert!(a.high_bid <= Money::from_pounds(15_000));
                    }
                }
                Action::Raise(p, by, ceiling) => {
                    let ceiling = Money::from_pounds(ceiling);
                    if house.raise(PlayerId(p), Money::from_pounds(by), ceiling, now, &bidders).is_ok() {
                        prop_assert!(house.current().unwrap().high_bid <= ceiling);
                    }
                }
                Action::Pass(p) => {
                    let _ = house.pass(PlayerId(p), now, &bidders);
                }
                Action::Wait(ticks) => {
                    for _ in 0..ticks {
                        now += 1;
                        house.advance(now, &bidders);
                    }
                }
            }
            prop_assert!(house.history().len() + usize::from(house.is_busy()) <= opened);
        }
    }
}
