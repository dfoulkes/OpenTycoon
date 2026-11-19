//! The world: owns all game state and runs the tick in a fixed order.
//!
//! # Tick order (authoritative; mirrored in docs/architecture.md)
//!
//! Each call to [`World::tick`] runs these phases, always in this order:
//!
//! 1. **Clock** advances one tick and reports what fires ([`ClockEvents`]).
//! 2. **Commands** queued by players and AIs since the last tick are applied, in submission
//!    order.
//! 3. **Midnight reckoning** (if midnight): net worth (R31), stock write-off (R1), solvency
//!    and bankruptcy (R3, R4), lease expiry (R9), credit-limit step (R7), events and
//!    front page (§9), population year step (R61).
//! 4. **Arrivals** (if 05:30): new population arrives and house-hunts (R53).
//! 5. **Settlement** (if 06:00): rent, utilities and stock bills paid (R32), tomorrow's order
//!    delivered (R2), population joins the map.
//! 6. **Auctions** advance countdowns and turn timers (§5.1).
//! 7. **AI** looks at the world and queues commands for the next tick (§10).
//! 8. **Customers** spawn, choose shops, walk and buy (§8).
//! 9. **Businesses** record sales and reputation (§7, §8.3).
//!
//! Every phase is deterministic given the state and the world's single [`SimRng`].

use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

use crate::ai::{AiSeat, Brain};
use crate::auctions::AuctionHouse;
use crate::businesses::{BusinessId, Businesses};
use crate::clock::{Clock, ClockEvents, ClockRules};
use crate::config::Config;
use crate::customers::Customers;
use crate::economy::{AccountId, Ledger, PlayerId, Solvency, SolvencyTracker};
use crate::events::Events;
use crate::hash::StableHasher;
use crate::land::{BlockId, Map};
use crate::money::{Money, Rate};
use crate::population::Population;
use crate::rng::{SimRng, seeded};
use crate::utilities::Utilities;

/// Something a player (human or AI) asks the world to do. Clients only ever send commands; the
/// world decides what happens (R40). Wire forms live in the `protocol` crate.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Command {
    /// Open an auction on an unowned block with an opening bid (§5.1, R23).
    OpenLandAuction {
        /// Block.
        block: BlockId,
        /// Opening bid; at least the block's land base price.
        bid: Money,
    },
    /// Raise the current bid by one of the configured increments (§5.1).
    Raise {
        /// The increment.
        by: Money,
    },
    /// Pass this round (R8).
    Pass,
    /// Set a good's price as a share of its reference price (§7.1).
    SetPrice {
        /// Business.
        business: BusinessId,
        /// Good id.
        good: String,
        /// Price as a share of `ref_price`.
        price: Rate,
    },
    /// Set tomorrow's order for a good (R2).
    SetOrder {
        /// Business.
        business: BusinessId,
        /// Good id.
        good: String,
        /// Units.
        quantity: i64,
    },
}

/// A player's status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PlayerStatus {
    /// In the game.
    Active,
    /// Out of the game; assets went to the city (R4).
    Bankrupt,
}

/// All game state. Construct with [`World::new`]; advance with [`World::tick`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct World {
    config: Config,
    seed: u64,
    rng: SimRng,
    clock: Clock,
    ledger: Ledger,
    solvency: SolvencyTracker,
    players: Vec<(PlayerId, PlayerStatus)>,
    map: Map,
    auctions: AuctionHouse,
    businesses: Businesses,
    utilities: Utilities,
    population: Population,
    customers: Customers,
    events: Events,
    ai: Vec<AiSeat>,
    queued: Vec<(PlayerId, Command)>,
}

impl World {
    /// A new game from validated config: `players` seats, the last `ai` of them played by the
    /// AI opponents listed in `ai.yaml`, all randomness from `seed`.
    pub fn new(config: Config, seed: u64, players: u8) -> Self {
        let clock = Clock::new(ClockRules::from_config(&config.game.clock));
        let map = Map::from_config(&config.map);
        let mut ledger = Ledger::new();
        ledger.open(AccountId::City, Money::ZERO);
        let seats: Vec<(PlayerId, PlayerStatus)> = (0..players)
            .map(|p| (PlayerId(p), PlayerStatus::Active))
            .collect();
        for (p, _) in &seats {
            ledger.open(
                AccountId::Player(*p),
                config.game.players.starting_cash.money(),
            );
        }
        let ai = config
            .ai
            .opponents
            .iter()
            .zip((0..players).rev())
            .map(|(personality, p)| AiSeat {
                player: PlayerId(p),
                personality: personality.clone(),
            })
            .collect();
        World {
            seed,
            rng: seeded(seed),
            clock,
            ledger,
            solvency: SolvencyTracker::default(),
            players: seats,
            map,
            auctions: AuctionHouse::default(),
            businesses: Businesses::default(),
            utilities: Utilities::default(),
            population: Population::default(),
            customers: Customers::default(),
            events: Events::default(),
            ai,
            queued: Vec::new(),
            config,
        }
    }

    /// Queues a command; it is applied at phase 2 of the next tick.
    pub fn submit(&mut self, player: PlayerId, command: Command) {
        self.queued.push((player, command));
    }

    /// Runs one tick in the documented order and returns what the clock fired.
    pub fn tick(&mut self) -> ClockEvents {
        let fired = self.clock.advance();
        self.apply_commands();
        if fired.midnight {
            self.midnight_reckoning();
        }
        if fired.arrivals {
            self.arrivals();
        }
        if fired.settlement {
            self.settlement();
        }
        self.auctions_phase();
        self.ai_phase();
        self.customers_phase();
        self.businesses_phase();
        fired
    }

    /// Phase 2. M0: commands are drained in order; each module owner wires its own.
    fn apply_commands(&mut self) {
        for (_player, _command) in std::mem::take(&mut self.queued) {}
    }

    /// Phase 3. M0: solvency only (R3, R4 strike counting).
    fn midnight_reckoning(&mut self) {
        let rules = &self.config.game.bankruptcy;
        for (player, status) in &mut self.players {
            if *status != PlayerStatus::Active {
                continue;
            }
            let balance = self.ledger.balance(AccountId::Player(*player));
            let outcome = self.solvency.check(
                *player,
                balance,
                rules.solvent_at_or_above.money(),
                rules.strikes_to_bankrupt,
            );
            if outcome == Solvency::Bankrupt {
                *status = PlayerStatus::Bankrupt;
            }
        }
    }

    /// Phase 4. Owner: population.
    #[allow(
        clippy::unused_self,
        reason = "M0 phase stub; the module owner fills it in"
    )]
    fn arrivals(&mut self) {}

    /// Phase 5. Owner: economy (bills from land, utilities, businesses).
    #[allow(
        clippy::unused_self,
        reason = "M0 phase stub; the module owner fills it in"
    )]
    fn settlement(&mut self) {}

    /// Phase 6. Owner: auctions.
    #[allow(
        clippy::unused_self,
        reason = "M0 phase stub; the module owner fills it in"
    )]
    fn auctions_phase(&mut self) {}

    /// Phase 7. AIs see the world read-only and queue commands for the next tick.
    fn ai_phase(&mut self) {
        let mut seats = std::mem::take(&mut self.ai);
        let mut rng = self.rng.clone();
        let mut out = Vec::new();
        for seat in &mut seats {
            let me = seat.player;
            out.extend(seat.decide(self, me, &mut rng).into_iter().map(|c| (me, c)));
        }
        self.rng = rng;
        self.ai = seats;
        self.queued.extend(out);
    }

    /// Phase 8. Owner: customers.
    #[allow(
        clippy::unused_self,
        reason = "M0 phase stub; the module owner fills it in"
    )]
    fn customers_phase(&mut self) {}

    /// Phase 9. Owner: businesses.
    #[allow(
        clippy::unused_self,
        reason = "M0 phase stub; the module owner fills it in"
    )]
    fn businesses_phase(&mut self) {}

    /// The config this game started with (§10b: never changes mid-game).
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// The seed.
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// The clock.
    pub fn clock(&self) -> &Clock {
        &self.clock
    }

    /// The ledger.
    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    /// The map.
    pub fn map(&self) -> &Map {
        &self.map
    }

    /// The auction house.
    pub fn auctions(&self) -> &AuctionHouse {
        &self.auctions
    }

    /// Businesses.
    pub fn businesses(&self) -> &Businesses {
        &self.businesses
    }

    /// Utilities.
    pub fn utilities(&self) -> &Utilities {
        &self.utilities
    }

    /// Population counts.
    pub fn population(&self) -> &Population {
        &self.population
    }

    /// Customers on the map.
    pub fn customers(&self) -> &Customers {
        &self.customers
    }

    /// Events and the front page.
    pub fn events(&self) -> &Events {
        &self.events
    }

    /// Player seats and status.
    pub fn players(&self) -> &[(PlayerId, PlayerStatus)] {
        &self.players
    }

    /// A portable hash of all mutable state, for determinism checks. Equal seeds, config and
    /// commands give equal hashes on every OS and CPU.
    pub fn state_hash(&self) -> u64 {
        let mut h = StableHasher::default();
        self.seed.hash(&mut h);
        self.rng.get_word_pos().hash(&mut h);
        self.clock.hash(&mut h);
        self.ledger.hash(&mut h);
        self.solvency.hash(&mut h);
        self.players.hash(&mut h);
        self.map.hash(&mut h);
        self.auctions.hash(&mut h);
        self.businesses.hash(&mut h);
        self.utilities.hash(&mut h);
        self.population.hash(&mut h);
        self.customers.hash(&mut h);
        self.events.hash(&mut h);
        self.ai.hash(&mut h);
        self.queued.hash(&mut h);
        h.finish()
    }
}
