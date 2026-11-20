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
use crate::businesses::{BuildRequest, BusinessId, Businesses, FitOutLevel, LeverError};
use crate::clock::{Clock, ClockEvents, ClockRules};
use crate::config::Config;
use crate::customers::Customers;
use crate::economy::{
    self, AccountId, Ledger, NetWorthBoard, PlayerId, Solvency, SolvencyTracker,
    credit_limit_on_day, spendable,
};
use crate::events::Events;
use crate::hash::StableHasher;
use crate::land::{BlockId, Footprint, Map};
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
    /// Sell a business to the city at once for `sell_to_city_ratio` of its build cost (R5).
    SellToCity {
        /// The business; the seller must own it.
        business: BusinessId,
    },
    /// Open or shut a shop (§7.1).
    SetOpen {
        /// Business.
        business: BusinessId,
        /// Open?
        open: bool,
    },
    /// Put up a building on a lot you lease (§7.1; R13, R22, R58). Build cost is paid at once
    /// and must fit within `balance + credit limit` (R7).
    Build {
        /// Block.
        block: BlockId,
        /// Business type id.
        type_id: String,
        /// Footprint in the lot.
        footprint: Footprint,
        /// Height in units.
        height: u32,
        /// Fit-out.
        fit_out: FitOutLevel,
    },
}

/// Why a businesses command was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BusinessCommandError {
    /// The player doesn't own that business.
    NotOwner,
    /// The player doesn't hold the lease on that block.
    NotLeaseholder,
    /// The build cost is more than `balance + credit limit` (R7).
    CannotAfford,
    /// A building rule refused it.
    Build(crate::businesses::BuildError),
    /// A lever refused it.
    Lever(LeverError),
}

impl From<LeverError> for BusinessCommandError {
    fn from(e: LeverError) -> Self {
        BusinessCommandError::Lever(e)
    }
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
    net_worth: NetWorthBoard,
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
            net_worth: NetWorthBoard::default(),
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

    /// Phase 2. Commands are applied in submission order; each module owner wires its own.
    /// A bankrupt player is out of the game, so their commands are dropped (R4).
    fn apply_commands(&mut self) {
        for (player, command) in std::mem::take(&mut self.queued) {
            if !self.is_active(player) {
                continue;
            }
            if matches!(
                command,
                Command::SetPrice { .. }
                    | Command::SetOrder { .. }
                    | Command::SetOpen { .. }
                    | Command::Build { .. }
            ) {
                // A refused lever or build changes nothing. Reporting rejections to the client
                // is the protocol's job (M2).
                let _ = self.apply_business_command(player, command);
                continue;
            }
            if let Command::SellToCity { business } = command {
                // A refused sale (not the seller's, no such business) changes nothing.
                let _ = economy::bankruptcy::sell_to_city(
                    player,
                    business,
                    &mut self.ledger,
                    &mut self.businesses,
                    &self.config,
                );
            }
        }
    }

    /// Phase 3, economy's part, in the spec's order: net worth (R31, scored for the day that
    /// just ended, so before the write-off) → stock write-off (R1, businesses) → solvency and
    /// bankruptcy hand-over (R3, R4). The credit-limit step (R7) needs no work here: the limit
    /// is derived from the day. Lease expiry, events and population follow (their owners).
    fn midnight_reckoning(&mut self) {
        self.score_net_worth();
        crate::businesses::write_off_stock(&mut self.businesses);
        self.businesses.lock_orders(); // R2: tomorrow's order locks at midnight
        self.check_solvency();
    }

    /// Net worth for every active player (R19, R31, R39). The clock has already stepped, so
    /// leases are valued at the year of the day being scored.
    fn score_net_worth(&mut self) {
        let years_per_day = i32::try_from(self.clock.rules().years_per_day).unwrap_or(0);
        let scoring_year = self.clock.year().saturating_sub(years_per_day);
        let uplift = self.config.game.net_worth.lot_property_uplift.rate();
        for (player, status) in &self.players {
            if *status != PlayerStatus::Active {
                continue;
            }
            let inputs = economy::net_worth::net_worth_inputs(
                AccountId::Player(*player),
                &self.ledger,
                &self.map,
                &self.businesses,
                &self.config.businesses.goods,
                scoring_year,
            );
            self.net_worth
                .record(*player, economy::net_worth(&inputs, uplift));
        }
    }

    /// The midnight solvency check (R3) and, on the last strike, the hand-over of every
    /// asset to the city (R4). The city is never checked: it can't go bust.
    fn check_solvency(&mut self) {
        let rules = &self.config.game.bankruptcy;
        for (player, status) in &mut self.players {
            if *status != PlayerStatus::Active {
                continue;
            }
            let account = AccountId::Player(*player);
            let outcome = self.solvency.check(
                *player,
                self.ledger.balance(account),
                rules.solvent_at_or_above.money(),
                rules.strikes_to_bankrupt,
            );
            if outcome == Solvency::Bankrupt {
                *status = PlayerStatus::Bankrupt;
                let handover = economy::bankruptcy::assets_of(account, &self.map, &self.businesses);
                economy::bankruptcy::hand_over_to_city(
                    &handover,
                    &mut self.map,
                    &mut self.businesses,
                    &self.config,
                );
            }
        }
    }

    /// Phase 4. Owner: population.
    #[allow(
        clippy::unused_self,
        reason = "M0 phase stub; the module owner fills it in"
    )]
    fn arrivals(&mut self) {}

    /// Phase 5, economy's part: overdraft interest (0 by default, R32), then every building's
    /// rent, utilities and stock order, paid with no limit check (R32). Delivery of the order
    /// (R2) and the population joining the map follow (their owners).
    fn settlement(&mut self) {
        let active = self
            .players
            .iter()
            .filter(|(_, s)| *s == PlayerStatus::Active)
            .map(|(p, _)| *p);
        let mut bills = economy::settlement::interest_bills(
            &self.ledger,
            active,
            self.config.game.overdraft.interest_rate.rate(),
        );
        bills.extend(economy::settlement::settlement_bills(
            &self.map,
            &self.businesses,
            &self.utilities,
            &self.config,
            self.clock.year(),
        ));
        self.ledger.settle(&bills);
        // R2: exactly the order just billed arrives now.
        self.businesses.deliver_orders();
    }

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

    // --- businesses ---------------------------------------------------------------------------

    /// Applies one businesses command for `player`. Levers need the player to own the
    /// business; building needs them to hold the lease and afford the cost (R7).
    fn apply_business_command(
        &mut self,
        player: PlayerId,
        command: Command,
    ) -> Result<(), BusinessCommandError> {
        let me = AccountId::Player(player);
        let owns = |b: &Businesses, id: BusinessId| b.get(id).is_some_and(|x| x.owner == me);
        match command {
            Command::SetPrice {
                business,
                good,
                price,
            } if owns(&self.businesses, business) => {
                self.businesses.set_price(business, &good, price)?;
            }
            Command::SetOrder {
                business,
                good,
                quantity,
            } if owns(&self.businesses, business) => {
                self.businesses.set_order(business, &good, quantity)?;
            }
            Command::SetOpen { business, open } if owns(&self.businesses, business) => {
                self.businesses.set_open(business, open)?;
            }
            Command::Build {
                block,
                type_id,
                footprint,
                height,
                fit_out,
            } => {
                let holds = self
                    .map
                    .block(block)
                    .and_then(|b| b.lease.as_ref())
                    .is_some_and(|l| l.holder == me);
                if !holds {
                    return Err(BusinessCommandError::NotLeaseholder);
                }
                let request = BuildRequest {
                    type_id,
                    block,
                    footprint,
                    height,
                    owner: me,
                    fit_out,
                };
                let year = self.clock.year();
                let cost = self
                    .businesses
                    .quote_build(&request, &self.config, &self.map, year)
                    .map_err(BusinessCommandError::Build)?;
                if spendable(self.ledger.balance(me), self.credit_limit()) < cost {
                    return Err(BusinessCommandError::CannotAfford);
                }
                let (_, cost) = self
                    .businesses
                    .build(request, &self.config, &self.map, year)
                    .map_err(BusinessCommandError::Build)?;
                self.ledger.transfer(me, AccountId::City, cost);
            }
            _ => return Err(BusinessCommandError::NotOwner),
        }
        Ok(())
    }

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

    /// Today's credit limit, the same for every player: `start + day × increase` (R7).
    pub fn credit_limit(&self) -> Money {
        credit_limit_on_day(&self.config.game.overdraft, self.clock.day())
    }

    /// A player's bid ceiling, `balance + credit limit` (R7, §5.1): the most they can bid or
    /// spend right now. `None` if the seat doesn't exist or the player is bankrupt (R4).
    /// May be negative after the 06:00 bills (R32).
    pub fn bid_ceiling(&self, player: PlayerId) -> Option<Money> {
        self.is_active(player).then(|| {
            spendable(
                self.ledger.balance(AccountId::Player(player)),
                self.credit_limit(),
            )
        })
    }

    /// True if an active player can pay `amount` without going past their credit limit (R7).
    pub fn can_afford(&self, player: PlayerId, amount: Money) -> bool {
        self.bid_ceiling(player).is_some_and(|c| amount <= c)
    }

    /// True if `player` is a seat in this game and not bankrupt.
    pub fn is_active(&self, player: PlayerId) -> bool {
        self.players
            .iter()
            .any(|(p, s)| *p == player && *s == PlayerStatus::Active)
    }

    /// A player's consecutive failed midnights (§2.3).
    pub fn strikes(&self, player: PlayerId) -> u32 {
        self.solvency.strikes(player)
    }

    /// The nightly leaderboard: every player's net worth at the last midnight. After the
    /// final midnight this is the final score (R19, R31).
    pub fn net_worth(&self) -> &NetWorthBoard {
        &self.net_worth
    }

    /// Mutable ledger, for scenario setup and tools. Play changes money through commands.
    #[doc(hidden)]
    pub fn ledger_mut(&mut self) -> &mut Ledger {
        &mut self.ledger
    }

    /// Mutable map, for scenario setup and tools. Play changes leases through auctions.
    #[doc(hidden)]
    pub fn map_mut(&mut self) -> &mut Map {
        &mut self.map
    }

    /// Mutable businesses, for scenario setup and tools. Play builds through commands.
    #[doc(hidden)]
    pub fn businesses_mut(&mut self) -> &mut Businesses {
        &mut self.businesses
    }

    /// The seats played by the AI opponents from `ai.yaml` (the rest are humans, or harness
    /// brains in `sim-cli batch`).
    pub fn ai_seats(&self) -> &[AiSeat] {
        &self.ai
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
        self.net_worth.hash(&mut h);
        h.finish()
    }
}

#[cfg(test)]
mod businesses_tests {
    //! The businesses commands and hooks wired through the real tick.
    use super::*;
    use crate::config::{ConfigFile, ConfigSources};
    use crate::land::Lease;

    fn shipped() -> Config {
        let text = |f: ConfigFile| match f {
            ConfigFile::Game => include_str!("../../../config/game.yaml"),
            ConfigFile::Map => include_str!("../../../config/map.yaml"),
            ConfigFile::Auctions => include_str!("../../../config/auctions.yaml"),
            ConfigFile::Utilities => include_str!("../../../config/utilities.yaml"),
            ConfigFile::Businesses => include_str!("../../../config/businesses.yaml"),
            ConfigFile::Customers => include_str!("../../../config/customers.yaml"),
            ConfigFile::Events => include_str!("../../../config/events.yaml"),
            ConfigFile::Ai => include_str!("../../../config/ai.yaml"),
        };
        let mut sources = ConfigSources::new();
        for f in ConfigFile::ALL {
            sources.insert(f.file_name(), text(f));
        }
        Config::load(&sources).unwrap_or_else(|e| panic!("{e}"))
    }

    const P0: PlayerId = PlayerId(0);
    const ME: AccountId = AccountId::Player(P0);

    fn world_with_lease() -> World {
        let mut w = World::new(shipped(), 7, 2);
        w.ai.clear();
        w.map.blocks[0].lease = Some(Lease {
            holder: ME,
            start_year: 1930,
            term_years: 30,
            last_price: Money::ZERO,
        });
        w
    }

    fn bakery() -> Command {
        Command::Build {
            block: BlockId(0),
            type_id: "bakery".into(),
            footprint: Footprint {
                x: 0,
                y: 0,
                w: 3,
                d: 3,
            },
            height: 10,
            fit_out: FitOutLevel::Medium,
        }
    }

    fn run_to(w: &mut World, second: u32) {
        loop {
            w.tick();
            if w.clock.second_of_day() == second {
                break;
            }
        }
    }

    #[test]
    fn a_leaseholder_builds_orders_receives_and_bins_stock() {
        let mut w = world_with_lease();
        let start = w.ledger.balance(ME);
        w.submit(P0, bakery());
        w.tick();
        let id = BusinessId(0);
        let cost = w.businesses.get(id).map(|b| b.build_cost).unwrap();
        // Old Kent Road is Low (×0.6): 90 × £10 × 0.6 = £540.
        assert_eq!(cost, Money::from_pounds(540));
        assert_eq!(w.ledger.balance(ME), start - cost);
        let total = w.ledger.total();

        w.submit(
            P0,
            Command::SetOrder {
                business: id,
                good: "bread".into(),
                quantity: 40,
            },
        );
        run_to(&mut w, 6 * 3600); // day 0, 06:00: nothing locked yet (R2)
        assert_eq!(w.businesses.get(id).unwrap().stock_of("bread"), 0);
        run_to(&mut w, 6 * 3600); // day 1, 06:00
        assert_eq!(w.businesses.get(id).unwrap().stock_of("bread"), 40);
        // Two mornings of utilities at £4.50 (costs.feature), one delivery of 40 × 4p = £1.60.
        assert_eq!(
            w.ledger.balance(ME),
            start - cost - Money::from_pence(450 + 450 + 160)
        );
        assert_eq!(w.ledger.total(), total, "settlement conserves money");
        run_to(&mut w, 0);
        assert_eq!(w.businesses.get(id).unwrap().stock_of("bread"), 0);
    }

    #[test]
    fn building_needs_the_lease_and_levers_need_ownership() {
        let mut w = world_with_lease();
        w.submit(PlayerId(1), bakery());
        w.tick();
        assert!(w.businesses.by_id.is_empty(), "player 1 holds no lease");
        w.submit(P0, bakery());
        w.tick();
        w.submit(
            PlayerId(1),
            Command::SetOpen {
                business: BusinessId(0),
                open: false,
            },
        );
        w.tick();
        assert!(w.businesses.get(BusinessId(0)).unwrap().open);
    }

    #[test]
    fn building_is_refused_beyond_the_credit_limit() {
        let mut w = world_with_lease();
        let poor = -(w.ledger.balance(ME) + Money::from_pounds(10_000));
        w.ledger.transfer(ME, AccountId::City, -poor);
        w.submit(P0, bakery());
        w.tick();
        assert!(w.businesses.by_id.is_empty());
    }
}
