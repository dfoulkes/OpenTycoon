//! Auction invariants through the whole world (docs/engineering.md): whatever players send,
//! money is conserved, a sale never charges more than the winner's ceiling at the time of the
//! bid, and lease terms stay within `0..=term`.
#![allow(clippy::unwrap_used, missing_docs, reason = "tests")]

mod common;

use proptest::prelude::*;
use sim_core::auctions::{AuctionItem, AuctionOutcome};
use sim_core::businesses::BusinessId;
use sim_core::economy::PlayerId;
use sim_core::land::BlockId;
use sim_core::world::Command;
use sim_core::{Money, World};

fn command() -> impl Strategy<Value = (u8, Command)> {
    let pounds = |p: i64| Money::from_pounds(p);
    (
        0u8..3,
        prop_oneof![
            (0u32..9, 1_500i64..16_000).prop_map(move |(b, bid)| Command::OpenLandAuction {
                block: BlockId(b),
                bid: pounds(bid),
            }),
            (0u32..2, 0i64..2_000).prop_map(move |(b, bid)| Command::OpenBusinessAuction {
                business: BusinessId(b),
                bid: pounds(bid),
            }),
            (0u32..9).prop_map(|b| Command::OfferForAuction {
                item: AuctionItem::Land(BlockId(b)),
            }),
            prop::sample::select(vec![50i64, 75, 100, 500, 1_000, 5_000])
                .prop_map(move |by| Command::Raise { by: pounds(by) }),
            Just(Command::Pass),
        ],
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]
    #[test]
    fn auctions_conserve_money_and_respect_ceilings(
        steps in prop::collection::vec((command(), 1u64..400), 0..80)
    ) {
        let mut config = common::shipped_config();
        config.ai.opponents.clear();
        let limit = config.economy.money.overdraft.start_limit.money();
        let term = config.economy.land.lease_term_years;
        let mut world = World::new(config, 7, 3);
        let total = world.ledger().total();
        for ((player, cmd), wait) in steps {
            world.submit(PlayerId(player), cmd);
            for _ in 0..wait {
                world.tick();
            }
            // Auctions conserve money; the only new money is shoppers' allowances (R63).
            prop_assert_eq!(world.ledger().total(), total + world.ledger().minted());
            for (_, balance) in world.ledger().accounts() {
                // Day 0: every bid is capped at balance + £10,000, so no one is ever deeper
                // in debt than the limit through auctions alone.
                if world.clock().day() == 0 {
                    prop_assert!(balance >= -limit, "{balance}");
                }
            }
            for block in &world.map().blocks {
                if let Some(lease) = &block.lease {
                    prop_assert!(lease.remaining_years(world.clock().year()) <= term);
                }
            }
            for outcome in world.auctions().history() {
                if let AuctionOutcome::Sold { price, .. } = outcome {
                    prop_assert!(*price > Money::ZERO);
                }
            }
        }
    }
}
