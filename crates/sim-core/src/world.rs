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
//!    front page: the election, history, rationing caps on the locked orders (§9, §9.1),
//!    population year step (R61).
//! 4. **Arrivals** (if 05:30): new population arrives and house-hunts (R53).
//! 5. **Settlement** (if 06:00): residents' rent from the shoppers' purse to their landlords
//!    (R51, R63), then land rent, utilities, insurance and stock bills paid (R32, R56, §9),
//!    tomorrow's order delivered (R2), random building events rolled (§9), population joins
//!    the map.
//! 6. **Auctions** advance countdowns and turn timers (§5.1).
//! 7. **AI** looks at the world and queues commands for the next tick (§10).
//! 8. **Customers** spawn, choose shops, walk and buy (§8).
//! 9. **Businesses** record sales and reputation (§7, §8.3).
//!
//! Every phase is deterministic given the state and the world's single [`SimRng`].

use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

use crate::ai::AiSeat;
use crate::auctions::{
    AuctionError, AuctionHouse, AuctionItem, AuctionOutcome, AuctionRules, Opening, QueuedSale,
};
use crate::businesses::{BuildRequest, Business, BusinessId, Businesses, FitOutLevel, LeverError};
use crate::clock::{Clock, ClockEvents, ClockRules};
use crate::config::Config;
use crate::customers::{Customers, PhaseInputs};
use crate::economy::{
    self, AccountId, Ledger, NetWorthBoard, PlayerId, Solvency, SolvencyTracker,
    credit_limit_on_day, spendable,
};
use crate::events::Events;
use crate::hash::StableHasher;
use crate::land::{BlockId, Footprint, Lease, Map};
use crate::money::{Money, Rate};
use crate::population::Population;
use crate::population::residential::{self, RentDue};
use crate::rng::{SimRng, seeded};
use crate::utilities::{Utilities, UtilityKind};

mod events_phase;

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
    /// Open an auction on a city-owned business with an opening bid of at least its property
    /// value (R5, R28).
    OpenBusinessAuction {
        /// Business.
        business: BusinessId,
        /// Opening bid.
        bid: Money,
    },
    /// Put your own lease or business up for auction (R5, R33). There is no opening bid;
    /// bidding starts from the reserve (the land base price for land, £0 for a business).
    OfferForAuction {
        /// Your lease (`Land`) or business.
        item: AuctionItem,
    },
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
    /// Set the rent per flat per day of an apartment block you own (R51).
    SetRent {
        /// The apartment block.
        building: BusinessId,
        /// Rent per flat per day.
        rent: Money,
    },
    /// Open an auction on an unheld utility with an opening bid of at least its reserve price
    /// (R12: auctioned like land).
    OpenUtilityAuction {
        /// The utility.
        utility: UtilityKind,
        /// Opening bid.
        bid: Money,
    },
    /// Take out or cancel insurance on all your buildings against random events (§9).
    SetInsurance {
        /// Insured?
        insured: bool,
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

/// One phase of the tick, in tick order (module docs; docs/architecture.md "The tick").
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// 1. Clock.
    Clock,
    /// 2. Commands.
    Commands,
    /// 3. Midnight reckoning (00:00 only).
    Midnight,
    /// 4. Arrivals (05:30 only).
    Arrivals,
    /// 5. Settlement (06:00 only).
    Settlement,
    /// 6. Auctions.
    Auctions,
    /// 7. AI.
    Ai,
    /// 8. Customers.
    Customers,
    /// 9. Businesses.
    Businesses,
}

impl Phase {
    /// Every phase, in tick order.
    pub const ALL: [Phase; 9] = [
        Phase::Clock,
        Phase::Commands,
        Phase::Midnight,
        Phase::Arrivals,
        Phase::Settlement,
        Phase::Auctions,
        Phase::Ai,
        Phase::Customers,
        Phase::Businesses,
    ];
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
        let ai: Vec<(PlayerId, String)> = config
            .ai
            .opponents
            .iter()
            .zip((0..players).rev())
            .map(|(personality, p)| (PlayerId(p), personality.clone()))
            .collect();
        Self::seated(config, seed, players, &ai)
    }

    /// A new game whose AI seats are chosen by the caller: `players` seats, `ai` lists which
    /// seat plays which personality (in the order the AI phase runs them); the other seats are
    /// humans or harness brains. Personalities must exist in `ai.yaml`. Balance studies use it
    /// to rotate personalities through the seats (R65). [`World::new`] is this with the
    /// `ai.yaml` opponents in the last seats.
    pub fn seated(config: Config, seed: u64, players: u8, ai: &[(PlayerId, String)]) -> Self {
        let clock = Clock::new(ClockRules::from_config(&config.game.clock));
        let map = Map::from_config(&config.map);
        let auctions =
            AuctionHouse::new(AuctionRules::from_config(&config.auctions, clock.rules()));
        let businesses = city_apartments(&map, &config);
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
        let ai = ai
            .iter()
            .map(|(p, personality)| AiSeat::new(*p, personality.clone(), seed))
            .collect();
        World {
            seed,
            rng: seeded(seed),
            clock,
            ledger,
            solvency: SolvencyTracker::default(),
            players: seats,
            map,
            auctions,
            businesses,
            utilities: Utilities::default(),
            population: Population::new(&config.customers, config.game.clock.start_year),
            customers: Customers::default(),
            events: Events::new(seed),
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
        self.tick_observed(&mut |_| {})
    }

    /// [`World::tick`], calling `after(phase)` as each phase that ran finishes, in tick order.
    /// The observer cannot touch the world, so the tick is exactly [`World::tick`]'s. It exists
    /// for the harness's per-phase timing (`sim-cli bench --phases`): sim-core never reads the
    /// wall clock, the observer does. `tick` passes a no-op, which compiles away.
    pub fn tick_observed(&mut self, after: &mut impl FnMut(Phase)) -> ClockEvents {
        let fired = self.clock.advance();
        after(Phase::Clock);
        self.apply_commands();
        after(Phase::Commands);
        if fired.midnight {
            self.midnight_reckoning();
            // red: events_step not wired yet
            self.population_year_step();
            after(Phase::Midnight);
        }
        if fired.arrivals {
            self.arrivals();
            after(Phase::Arrivals);
        }
        if fired.settlement {
            self.settlement();
            // red: random_incidents not wired yet
            after(Phase::Settlement);
        }
        self.auctions_phase();
        after(Phase::Auctions);
        self.ai_phase();
        after(Phase::Ai);
        self.customers_phase();
        after(Phase::Customers);
        self.businesses_phase();
        after(Phase::Businesses);
        fired
    }

    /// Phase 2. Commands are applied in submission order; each module owner wires its own.
    /// A bankrupt player is out of the game, so their commands are dropped (R4).
    fn apply_commands(&mut self) {
        for (player, command) in std::mem::take(&mut self.queued) {
            if !self.is_active(player) {
                continue;
            }
            match command {
                Command::SellToCity { business } => {
                    // A refused sale (not the seller's, no such business) changes nothing.
                    let _ = economy::bankruptcy::sell_to_city(
                        player,
                        business,
                        &mut self.ledger,
                        &mut self.businesses,
                        &self.config,
                    );
                }
                Command::SetPrice { .. }
                | Command::SetOrder { .. }
                | Command::SetOpen { .. }
                | Command::Build { .. } => {
                    // A refused lever or build changes nothing. Reporting rejections to the
                    // client is the protocol's job (M2).
                    let _ = self.apply_business_command(player, command);
                }
                Command::OpenLandAuction { .. }
                | Command::OpenBusinessAuction { .. }
                | Command::OfferForAuction { .. }
                | Command::Raise { .. }
                | Command::Pass
                | Command::OpenUtilityAuction { .. } => {
                    self.apply_auction_command(player, &command);
                }
                Command::SetRent { .. } => self.apply_population_command(player, &command),
                Command::SetInsurance { insured } => self.set_insurance(player, insured),
            }
        }
    }

    /// Phase 2, population's commands: owner-set rent (R51). Refused changes are dropped.
    fn apply_population_command(&mut self, player: PlayerId, command: &Command) {
        if let Command::SetRent { building, rent } = command {
            let _refused = residential::set_rent(
                &mut self.population.residential,
                &self.businesses,
                &self.config.customers.residential.building_type,
                player,
                *building,
                *rent,
            );
        }
    }

    /// End of phase 3 (owner: population): the cohort model's year step (R61). The clock has
    /// already stepped, so `clock.year()` is the new year.
    fn population_year_step(&mut self) {
        let years = self.clock.rules().years_per_day;
        crate::population::year_step(
            &mut self.population,
            &self.config.customers,
            self.clock.year(),
            years,
            &mut self.rng,
        );
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
        self.lease_expiry();
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
                self.utilities.release(account); // R4: back to the city, open for bids
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

    /// Phase 4. Owner: population. The train's arrivals house-hunt, then the unhoused re-hunt
    /// and residents whose window is open may move (R53, R54).
    fn arrivals(&mut self) {
        let listings = residential::listings(
            &self.businesses,
            &self.map,
            &self.config,
            &self.population.residential,
        );
        crate::population::arrivals(
            &mut self.population,
            &listings,
            &self.config.customers,
            self.clock.year(),
            &mut self.rng,
        );
        // R63: today's allowances are new money, minted into the shoppers' purse.
        let allowances =
            crate::population::daily_allowances(&self.population, &self.config.customers);
        self.ledger.mint(AccountId::Shoppers, allowances);
    }

    /// Phase 5, economy's part: overdraft interest (0 by default, R32), residents' rent out of
    /// the shoppers' purse to each block's owner (R51, R63; §8.2: before anyone shops), then
    /// every building's rent, utilities and stock order (apartment blocks too, R56), paid with
    /// no limit check (R32). Delivery of the order (R2) and the population joining the map
    /// follow (their owners).
    fn settlement(&mut self) {
        let rent_due = self.rent_due();
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
        bills.extend(economy::settlement::resident_rent_bills(&rent_due));
        // red: insurance premiums not billed yet
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

    /// Phase 6. Owner: auctions. Countdowns, turn timers and closing (§5.1); the winner pays
    /// the seller at once and takes the lease or business.
    fn auctions_phase(&mut self) {
        if !self.auctions.is_busy() && self.auctions.queue().is_empty() {
            return;
        }
        let bidders = self.active_players();
        if let Some(outcome) = self.auctions.advance(self.clock.tick(), &bidders) {
            self.apply_auction_outcome(outcome);
        }
    }

    /// Phase 2, auction commands (§5.1): opening bids, offers, raises and passes. Other
    /// commands are left to their owners.
    fn apply_auction_command(&mut self, player: PlayerId, command: &Command) {
        let now = self.clock.tick();
        let result = match command {
            Command::OpenLandAuction { block, bid } => self.open_land_auction(player, *block, *bid),
            Command::OpenBusinessAuction { business, bid } => {
                self.open_business_auction(player, *business, *bid)
            }
            Command::OfferForAuction { item } => self.offer_for_auction(player, *item),
            Command::OpenUtilityAuction { utility, bid } => {
                self.open_utility_auction(player, *utility, *bid)
            }
            Command::Raise { by } => {
                let ceiling = self.bid_ceiling(player).unwrap_or(Money::ZERO);
                let bidders = self.active_players();
                self.auctions.raise(player, *by, ceiling, now, &bidders)
            }
            Command::Pass => {
                let bidders = self.active_players();
                self.auctions.pass(player, now, &bidders)
            }
            _ => return,
        };
        self.auctions.record(player, result);
    }

    /// A player's opening bid on an unleased block; the minimum is its land base price (R23).
    fn open_land_auction(
        &mut self,
        player: PlayerId,
        block: BlockId,
        bid: Money,
    ) -> Result<(), AuctionError> {
        let base = self
            .map
            .block(block)
            .ok_or(AuctionError::Unknown)?
            .land_base_price;
        if !self.map.is_open_for_bids(block) {
            return Err(AuctionError::NotForSale);
        }
        let opening = Opening {
            item: AuctionItem::Land(block),
            seller: AccountId::City,
            opening_bid: Some((player, bid)),
            minimum: base,
        };
        let ceiling = self.bid_ceiling(player).unwrap_or(Money::ZERO);
        self.auctions.open(opening, ceiling, self.clock.tick())
    }

    /// A player's opening bid on a city-owned business; the minimum is its property value,
    /// the build cost (R41). Player-owned businesses are sold only by their owner (R5).
    fn open_business_auction(
        &mut self,
        player: PlayerId,
        business: BusinessId,
        bid: Money,
    ) -> Result<(), AuctionError> {
        let b = self
            .businesses
            .by_id
            .get(&business)
            .ok_or(AuctionError::Unknown)?;
        if b.owner != AccountId::City {
            return Err(AuctionError::NotForSale);
        }
        let opening = Opening {
            item: AuctionItem::Business(business),
            seller: AccountId::City,
            opening_bid: Some((player, bid)),
            minimum: b.build_cost,
        };
        let ceiling = self.bid_ceiling(player).unwrap_or(Money::ZERO);
        self.auctions.open(opening, ceiling, self.clock.tick())
    }

    /// A player puts their own lease (R33) or business (R5) up for auction. Same queue, same
    /// countdown, no shortcuts.
    fn offer_for_auction(
        &mut self,
        player: PlayerId,
        item: AuctionItem,
    ) -> Result<(), AuctionError> {
        let me = AccountId::Player(player);
        let reserve = match item {
            AuctionItem::Land(block) => {
                let b = self.map.block(block).ok_or(AuctionError::Unknown)?;
                if b.lease.as_ref().is_none_or(|l| l.holder != me) {
                    return Err(AuctionError::NotYours);
                }
                b.land_base_price
            }
            AuctionItem::Business(id) => {
                let b = self
                    .businesses
                    .by_id
                    .get(&id)
                    .ok_or(AuctionError::Unknown)?;
                if b.owner != me {
                    return Err(AuctionError::NotYours);
                }
                Money::ZERO
            }
            AuctionItem::Utility(kind) => self.utility_offer_reserve(player, kind)?,
        };
        let opening = Opening {
            item,
            seller: me,
            opening_bid: None,
            minimum: reserve,
        };
        self.auctions.open(opening, Money::ZERO, self.clock.tick())
    }

    /// The winner pays the seller at once (§5.1) and takes the lease or the business. A lease
    /// sold by its holder keeps its remaining years (R33); one sold by the city starts a fresh
    /// term. Land auctions move only the lease; businesses only their owner (R5).
    fn apply_auction_outcome(&mut self, outcome: AuctionOutcome) {
        let AuctionOutcome::Sold {
            item,
            seller,
            winner,
            price,
        } = outcome
        else {
            return;
        };
        let buyer = AccountId::Player(winner);
        if self.utility_sale_void(item) {
            self.auctions.mark_defaulted();
            return;
        }
        // The ceiling was checked at each bid; if the winner has since gone bankrupt or the
        // 06:00 bills took them past it, the sale defaults and the item stays with its seller.
        let paid = self.is_active(winner)
            && self
                .ledger
                .pay_within_limit(buyer, seller, price, self.credit_limit())
                .is_ok();
        if !paid {
            self.auctions.mark_defaulted();
            return;
        }
        match item {
            AuctionItem::Land(block) => {
                let held = self
                    .map
                    .block(block)
                    .and_then(|b| b.lease.clone())
                    .filter(|l| l.holder == seller && seller != AccountId::City);
                let lease = held.map_or_else(
                    || {
                        Lease::new(
                            buyer,
                            self.clock.year(),
                            self.config.game.leases.term_years,
                            price,
                        )
                    },
                    |l| l.transferred(buyer, price),
                );
                self.map.grant_lease(block, lease);
            }
            AuctionItem::Business(id) => {
                if let Some(b) = self.businesses.by_id.get_mut(&id) {
                    b.owner = buyer;
                }
            }
            AuctionItem::Utility(_) => {} // red: utility sale not applied yet
        }
    }

    /// Midnight, after solvency: expired leases go straight back to auction (R9). Buildings
    /// stay and pay rent to the city until a new holder wins.
    fn lease_expiry(&mut self) {
        for block in self.map.expire_leases(self.clock.year()) {
            let reserve = self
                .map
                .block(block)
                .map_or(Money::ZERO, |b| b.land_base_price);
            self.auctions.enqueue(QueuedSale {
                item: AuctionItem::Land(block),
                seller: AccountId::City,
                reserve,
            });
        }
    }

    /// Players still in the game, in seat order.
    fn active_players(&self) -> Vec<PlayerId> {
        self.players
            .iter()
            .filter(|(_, s)| *s == PlayerStatus::Active)
            .map(|(p, _)| *p)
            .collect()
    }

    /// Phase 7. AIs see the world read-only and queue commands for the next tick. Each seat
    /// draws from its own RNG stream, never the world's, so an AI never shifts the world's
    /// draws. Bankrupt seats are out of the game (R4).
    fn ai_phase(&mut self) {
        if self.ai.is_empty() {
            return;
        }
        let mut seats = std::mem::take(&mut self.ai);
        let mut out = Vec::new();
        for seat in &mut seats {
            let me = seat.player;
            if self.is_active(me) {
                out.extend(seat.play(self).into_iter().map(|c| (me, c)));
            }
        }
        self.ai = seats;
        self.queued.extend(out);
    }

    /// Phase 8. Owner: customers. Shoppers spawn, choose, walk and buy (§8). Each sale moves
    /// money from the shoppers' purse (minted at 05:30, R63) to the shop's owner.
    fn customers_phase(&mut self) {
        let report = crate::customers::run_phase(
            &mut self.customers,
            PhaseInputs {
                config: &self.config,
                map: &self.map,
                clock: &self.clock,
                population: &self.population,
                modifiers: &self.events.modifiers,
            },
            &mut self.businesses,
            &mut self.rng,
        );
        for sale in &report.sales {
            if let Some(shop) = self.businesses.by_id.get(&sale.shop) {
                self.ledger
                    .transfer(AccountId::Shoppers, shop.owner, sale.amount);
            }
        }
    }

    /// Rent residents owe at the 06:00 settlement, per apartment block (R51, §7.2), each
    /// resident's capped at their allowance (R63). Economy settles it from the shoppers' purse
    /// to the block's owner.
    pub fn rent_due(&self) -> Vec<RentDue> {
        let listings = residential::listings(
            &self.businesses,
            &self.map,
            &self.config,
            &self.population.residential,
        );
        residential::rent_due(
            &self.population.residential,
            &listings,
            &self.config.customers,
        )
    }

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

/// The city's starting apartment blocks (R28), one in each Low-tier set, owned by the city at
/// their build cost (R41). Placed at world creation from `map.yaml` `city_apartments`.
fn city_apartments(map: &Map, config: &Config) -> Businesses {
    let mut businesses = Businesses::default();
    let type_id = &config.map.city_apartments.business_type;
    let Some(kind) = config.businesses.types.get(type_id) else {
        return businesses;
    };
    let rates = &config.businesses;
    for site in crate::land::city_apartment_sites(map, &config.map) {
        let Some(block) = map.block(site.block) else {
            continue;
        };
        let build_cost = crate::businesses::build_cost(
            site.height,
            site.footprint,
            rates.build_base_rate.money(),
            config.map.tiers.get(block.tier).desirability_mult.rate(),
            rates.fit_out.medium.build_cost_mult.rate(),
        );
        let request = BuildRequest {
            type_id: type_id.clone(),
            block: site.block,
            footprint: site.footprint,
            height: site.height,
            owner: AccountId::City,
            fit_out: FitOutLevel::Medium,
        };
        let id = BusinessId(businesses.next_id);
        businesses.next_id += 1;
        businesses
            .by_id
            .insert(id, Business::new(request, kind, build_cost));
    }
    businesses
}

#[cfg(test)]
mod businesses_tests {
    //! The businesses commands and hooks wired through the real tick.
    use super::*;
    use crate::config::{ConfigFile, ConfigSources};
    use crate::land::Lease;

    pub(super) fn shipped() -> Config {
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
        let id = w.businesses.owned_by(ME).next().map(|(id, _)| id).unwrap();
        let cost = w.businesses.get(id).map(|b| b.build_cost).unwrap();
        // Old Kent Road is Low (×0.6): 90 × £10 × 0.6 = £540.
        assert_eq!(cost, Money::from_pounds(540));
        assert_eq!(w.ledger.balance(ME), start - cost);
        let total = w.ledger.total();
        let minted = w.ledger.minted();

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
        // Settlement conserves money; the only new money is the shoppers' allowances (R63).
        assert_eq!(
            w.ledger.total(),
            total + (w.ledger.minted() - minted),
            "settlement conserves money"
        );
        run_to(&mut w, 0);
        assert_eq!(w.businesses.get(id).unwrap().stock_of("bread"), 0);
    }

    #[test]
    fn building_needs_the_lease_and_levers_need_ownership() {
        let mut w = world_with_lease();
        w.submit(PlayerId(1), bakery());
        w.tick();
        assert!(
            w.businesses
                .owned_by(AccountId::Player(PlayerId(1)))
                .next()
                .is_none(),
            "player 1 holds no lease"
        );
        w.submit(P0, bakery());
        w.tick();
        let id = w.businesses.owned_by(ME).next().map(|(id, _)| id).unwrap();
        w.submit(
            PlayerId(1),
            Command::SetOpen {
                business: id,
                open: false,
            },
        );
        w.tick();
        assert!(w.businesses.get(id).unwrap().open);
    }

    #[test]
    fn building_is_refused_beyond_the_credit_limit() {
        let mut w = world_with_lease();
        let poor = -(w.ledger.balance(ME) + Money::from_pounds(10_000));
        w.ledger.transfer(ME, AccountId::City, -poor);
        w.submit(P0, bakery());
        w.tick();
        assert!(w.businesses.owned_by(ME).next().is_none());
    }

    #[test]
    fn an_observed_tick_is_the_same_tick_and_reports_each_phase_that_ran_in_order() {
        let mut plain = World::new(shipped(), 7, 2);
        let mut observed = World::new(shipped(), 7, 2);
        let ticks_per_day = plain.clock.rules().ticks_per_day();
        let mut counts = std::collections::BTreeMap::<Phase, u32>::new();
        for _ in 0..ticks_per_day {
            let mut seen = Vec::new();
            let a = plain.tick();
            let b = observed.tick_observed(&mut |p| seen.push(p));
            assert_eq!(a, b);
            assert!(
                seen.windows(2).all(|w| w[0] < w[1]),
                "out of order: {seen:?}"
            );
            for p in seen {
                *counts.entry(p).or_default() += 1;
            }
        }
        assert_eq!(plain.state_hash(), observed.state_hash());
        for p in Phase::ALL {
            let once_a_day = matches!(p, Phase::Midnight | Phase::Arrivals | Phase::Settlement);
            let expected = if once_a_day { 1 } else { ticks_per_day };
            assert_eq!(counts.get(&p).copied(), Some(expected), "{p:?}");
        }
    }
}

#[cfg(test)]
mod ai_tests {
    //! The AI phase wired through the real tick (§10).
    use super::*;

    #[test]
    fn the_ai_phase_never_draws_from_the_worlds_rng() {
        let mut w = World::new(businesses_tests::shipped(), 7, 2);
        assert_eq!(w.ai_seats()[0].personality, "grocer");
        // Through the opening, the build-out and a day's trading.
        for _ in 0..(w.clock.rules().ticks_per_day() + 3_000) {
            w.tick();
            let before = w.rng.get_word_pos();
            w.ai_phase();
            assert_eq!(w.rng.get_word_pos(), before, "tick {}", w.clock.tick());
        }
        assert!(
            w.businesses
                .owned_by(AccountId::Player(PlayerId(1)))
                .next()
                .is_some(),
            "the Grocer built nothing"
        );
    }

    #[test]
    fn a_seated_world_puts_each_personality_where_it_is_asked() {
        let ai = [
            (PlayerId(0), "toff".to_owned()),
            (PlayerId(2), "grocer".to_owned()),
        ];
        let w = World::seated(businesses_tests::shipped(), 3, 3, &ai);
        let seats: Vec<(PlayerId, &str)> = w
            .ai_seats()
            .iter()
            .map(|s| (s.player, s.personality.as_str()))
            .collect();
        assert_eq!(seats, vec![(PlayerId(0), "toff"), (PlayerId(2), "grocer")]);
        assert_eq!(w.players().len(), 3);
    }

    #[test]
    fn new_is_seated_with_the_opponents_in_the_last_seats() {
        let config = businesses_tests::shipped();
        let mut a = World::new(config.clone(), 11, 2);
        let mut b = World::seated(config, 11, 2, &[(PlayerId(1), "grocer".to_owned())]);
        for _ in 0..20_000 {
            a.tick();
            b.tick();
        }
        assert_eq!(a.state_hash(), b.state_hash());
    }

    #[test]
    fn a_bankrupt_ai_seat_issues_nothing() {
        let mut w = World::new(businesses_tests::shipped(), 7, 2);
        w.players[1].1 = PlayerStatus::Bankrupt;
        w.ai_phase();
        assert_eq!(w.queued, Vec::new());
    }
}
