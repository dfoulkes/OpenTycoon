//! The `.otr` replay format: what a replay holds and its byte-level envelope
//! (docs/replay.md "Byte layout").
//!
//! A replay is a [`Header`], the command log ([`CommandRecord`]s), one [`Checkpoint`] per
//! midnight and an [`Outcome`]. On disk it is a 16-byte envelope header followed by a zstd frame
//! of the `postcard` encoding of [`Replay`] (ticks delta-encoded). Envelopes are
//! self-delimiting, so a segment file is just envelopes back to back.

use serde::{Deserialize, Serialize};
use sim_core::world::Command;

/// First four bytes of every envelope.
pub const MAGIC: [u8; 4] = *b"OTRP";

/// The format version this build writes and reads. Bump it on any change to the payload's
/// layout (docs/replay.md "Versioning").
pub const FORMAT_VERSION: u16 = 1;

/// Envelope header length in bytes.
pub const ENVELOPE_LEN: usize = 16;

/// Envelope flag: the payload is a zstd frame. Always set by this version.
pub const FLAG_ZSTD: u16 = 1;

/// zstd level used when writing.
pub const ZSTD_LEVEL: i32 = 9;

/// Who sits in a seat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SeatKind {
    /// A person, through a client.
    Human,
    /// A program: one of sim-core's AI personalities or a harness brain.
    Ai,
}

impl SeatKind {
    /// `human` or `ai`, as the manifest writes it.
    pub fn name(self) -> &'static str {
        match self {
            SeatKind::Human => "human",
            SeatKind::Ai => "ai",
        }
    }
}

/// One seat of a recorded game.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Seat {
    /// Human or AI.
    pub kind: SeatKind,
    /// What plays the seat: `core:<personality>` for sim-core's AI, `harness:<brain>` for a
    /// harness brain, `client` for a human.
    pub brain: String,
    /// The brain's version, or a hash of its settings (16 hex digits) for a sim-core
    /// personality.
    pub brain_version: String,
    /// Anonymous player id for a human seat; never a name. `None` for AI seats.
    pub player_id: Option<String>,
}

/// Prefix of a sim-core AI personality's brain id.
pub const CORE_BRAIN: &str = "core:";

impl Seat {
    /// The sim-core personality that plays this seat, if any.
    pub fn core_personality(&self) -> Option<&str> {
        self.brain.strip_prefix(CORE_BRAIN)
    }
}

/// Everything needed to rebuild the game's starting world, and where it came from.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Header {
    /// Unique within the run.
    pub game_id: String,
    /// The run (batch, study, server session) it belongs to.
    pub run_id: String,
    /// The sim build's git commit.
    pub git_commit: String,
    /// The build's rules fingerprint (16 hex digits): the state hash of the reference game.
    pub rules_fingerprint: String,
    /// Hash of the effective config (16 hex digits); the snapshot is `configs/<hash>.yaml`.
    pub config_hash: String,
    /// The game seed.
    pub seed: u64,
    /// The length preset the game ran (`""` if none).
    pub preset: String,
    /// First year.
    pub start_year: i32,
    /// Last year.
    pub end_year: i32,
    /// Seats, in player order.
    pub seats: Vec<Seat>,
    /// The seats sim-core's AI plays, in the order the AI phase runs them.
    pub ai_order: Vec<u8>,
}

/// Where a command came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Origin {
    /// Submitted to the world from outside: a client or a harness brain. Replays re-submit
    /// these.
    External,
    /// Queued by sim-core's AI phase. Replays regenerate these and check they match.
    Core,
}

/// One command the world applied.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CommandRecord {
    /// The tick it was applied on.
    pub tick: u64,
    /// The seat that sent it.
    pub seat: u8,
    /// Where it came from.
    pub origin: Origin,
    /// The world accepted it.
    pub accepted: bool,
    /// The command as submitted.
    pub command: Command,
}

/// One seat at a midnight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SeatCheckpoint {
    /// Net worth in pence, as the nightly board scored it.
    pub net_worth: i64,
    /// Cash in pence.
    pub cash: i64,
    /// Leases held.
    pub leases: u32,
    /// Businesses owned.
    pub businesses: u32,
    /// Bankrupt.
    pub bankrupt: bool,
}

/// The world at one midnight.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Midnights passed (1 = the first).
    pub day: u32,
    /// The tick that fired it.
    pub tick: u64,
    /// `World::state_hash()` after that tick.
    pub state_hash: u64,
    /// Per seat.
    pub seats: Vec<SeatCheckpoint>,
}

/// Who won.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Winner {
    /// One seat.
    Seat(u8),
    /// Several solvent seats shared the top net worth.
    Tie,
    /// Nobody was solvent.
    Nobody,
}

impl std::fmt::Display for Winner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Winner::Seat(p) => write!(f, "p{p}"),
            Winner::Tie => f.write_str("tie"),
            Winner::Nobody => f.write_str("none"),
        }
    }
}

/// One seat at the whistle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SeatOutcome {
    /// 1 = best. Solvent seats by net worth (ties share a rank), then bankrupt ones.
    pub rank: u8,
    /// Final net worth in pence.
    pub net_worth: i64,
    /// Went bankrupt.
    pub bankrupt: bool,
}

/// How the game ended.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Outcome {
    /// Winner.
    pub winner: Winner,
    /// Per seat.
    pub seats: Vec<SeatOutcome>,
    /// Midnights passed.
    pub days_played: u32,
    /// Ticks run.
    pub ticks: u64,
    /// `World::state_hash()` at the end.
    pub final_state_hash: u64,
}

/// A whole recorded game.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Replay {
    /// Header.
    pub header: Header,
    /// Every command the world applied, in order.
    pub commands: Vec<CommandRecord>,
    /// One per midnight, in order.
    pub checkpoints: Vec<Checkpoint>,
    /// The end.
    pub outcome: Outcome,
}

/// Why bytes are not a replay this build can read.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DecodeError {
    /// Fewer bytes than the envelope says.
    #[error("truncated replay: need {need} bytes, have {have}")]
    Truncated {
        /// Bytes needed.
        need: usize,
        /// Bytes present.
        have: usize,
    },
    /// Not an `.otr` envelope.
    #[error("not a replay (bad magic)")]
    Magic,
    /// Written by a different format version.
    #[error("replay format version {found}; this build reads version {FORMAT_VERSION}")]
    Version {
        /// The version found.
        found: u16,
    },
    /// Unknown flags.
    #[error("unsupported replay flags {0:#06x}")]
    Flags(u16),
    /// The payload's checksum does not match.
    #[error("replay checksum mismatch (corrupt payload)")]
    Checksum,
    /// The payload did not decompress or decode.
    #[error("replay payload: {0}")]
    Payload(String),
}

/// Why a replay could not be encoded.
#[derive(Debug, thiserror::Error)]
pub enum EncodeError {
    /// postcard or zstd failed.
    #[error("encoding replay: {0}")]
    Encode(String),
    /// The payload is over 4 GiB.
    #[error("replay payload too large: {0} bytes")]
    TooLarge(usize),
}

/// The payload as postcard sees it: the command log's ticks are deltas from the previous
/// command (the first from 0), so a log of many commands per day stays a byte or two per tick.
#[derive(Serialize, Deserialize)]
struct Wire {
    header: Header,
    commands: Vec<WireCommand>,
    checkpoints: Vec<Checkpoint>,
    outcome: Outcome,
}

#[derive(Serialize, Deserialize)]
struct WireCommand {
    dtick: u64,
    seat: u8,
    origin: Origin,
    accepted: bool,
    command: Command,
}

/// Encodes a replay as one envelope.
pub fn encode(replay: &Replay) -> Result<Vec<u8>, EncodeError> {
    let mut last = 0u64;
    let commands = replay
        .commands
        .iter()
        .map(|c| {
            // A log is in tick order; a decreasing tick would not round-trip, so refuse it.
            let dtick = c.tick.checked_sub(last).ok_or_else(|| {
                EncodeError::Encode(format!("command ticks go backwards at tick {}", c.tick))
            })?;
            last = c.tick;
            Ok(WireCommand {
                dtick,
                seat: c.seat,
                origin: c.origin,
                accepted: c.accepted,
                command: c.command.clone(),
            })
        })
        .collect::<Result<Vec<_>, EncodeError>>()?;
    let wire = Wire {
        header: replay.header.clone(),
        commands,
        checkpoints: replay.checkpoints.clone(),
        outcome: replay.outcome.clone(),
    };
    let raw = postcard::to_stdvec(&wire).map_err(|e| EncodeError::Encode(e.to_string()))?;
    let payload =
        zstd::bulk::compress(&raw, ZSTD_LEVEL).map_err(|e| EncodeError::Encode(e.to_string()))?;
    let len = u32::try_from(payload.len()).map_err(|_| EncodeError::TooLarge(payload.len()))?;
    let mut out = Vec::with_capacity(ENVELOPE_LEN + payload.len());
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&FLAG_ZSTD.to_le_bytes());
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&crc32fast::hash(&payload).to_le_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

fn le_u16(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn le_u32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// Upper bound on a decompressed payload (a sanity check against corrupt input).
const MAX_RAW: usize = 1 << 30;

/// Decodes the envelope at the start of `bytes`; returns the replay and the bytes it used.
pub fn decode_one(bytes: &[u8]) -> Result<(Replay, usize), DecodeError> {
    let truncated = |need: usize| DecodeError::Truncated {
        need,
        have: bytes.len(),
    };
    if bytes.len() < ENVELOPE_LEN {
        return Err(truncated(ENVELOPE_LEN));
    }
    if bytes[0..4] != MAGIC {
        return Err(DecodeError::Magic);
    }
    let version = le_u16(bytes, 4);
    if version != FORMAT_VERSION {
        return Err(DecodeError::Version { found: version });
    }
    let flags = le_u16(bytes, 6);
    if flags != FLAG_ZSTD {
        return Err(DecodeError::Flags(flags));
    }
    let len = le_u32(bytes, 8) as usize;
    let end = ENVELOPE_LEN + len;
    if bytes.len() < end {
        return Err(truncated(end));
    }
    let payload = &bytes[ENVELOPE_LEN..end];
    if crc32fast::hash(payload) != le_u32(bytes, 12) {
        return Err(DecodeError::Checksum);
    }
    let raw = zstd::bulk::decompress(payload, MAX_RAW)
        .map_err(|e| DecodeError::Payload(e.to_string()))?;
    let wire: Wire = postcard::from_bytes(&raw).map_err(|e| DecodeError::Payload(e.to_string()))?;
    let mut tick = 0u64;
    let mut commands = Vec::with_capacity(wire.commands.len());
    for c in wire.commands {
        tick = tick
            .checked_add(c.dtick)
            .ok_or_else(|| DecodeError::Payload("command tick overflows".into()))?;
        commands.push(CommandRecord {
            tick,
            seat: c.seat,
            origin: c.origin,
            accepted: c.accepted,
            command: c.command,
        });
    }
    let replay = Replay {
        header: wire.header,
        commands,
        checkpoints: wire.checkpoints,
        outcome: wire.outcome,
    };
    Ok((replay, end))
}

/// Decodes every envelope in a segment, with each one's byte offset and length.
pub fn decode_all(bytes: &[u8]) -> Result<Vec<(Replay, usize, usize)>, DecodeError> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let (replay, used) = decode_one(&bytes[at..])?;
        out.push((replay, at, used));
        at += used;
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use proptest::prelude::*;
    use sim_core::auctions::AuctionItem;
    use sim_core::businesses::{BusinessId, FitOutLevel};
    use sim_core::land::{BlockId, Footprint};
    use sim_core::{Money, Rate};

    pub(crate) fn command() -> impl Strategy<Value = Command> {
        let money = any::<i64>().prop_map(Money::from_pence);
        prop_oneof![
            (any::<u32>(), money.clone()).prop_map(|(b, bid)| Command::OpenLandAuction {
                block: BlockId(b),
                bid
            }),
            money.clone().prop_map(|by| Command::Raise { by }),
            Just(Command::Pass),
            (any::<u32>(), "[a-z_]{0,12}", any::<i64>()).prop_map(|(b, good, bp)| {
                Command::SetPrice {
                    business: BusinessId(b),
                    good,
                    price: Rate::from_bp(bp),
                }
            }),
            (any::<u32>(), "[a-z_]{0,12}", any::<i64>()).prop_map(|(b, good, quantity)| {
                Command::SetOrder {
                    business: BusinessId(b),
                    good,
                    quantity,
                }
            }),
            (any::<u32>(), any::<bool>()).prop_map(|(b, open)| Command::SetOpen {
                business: BusinessId(b),
                open
            }),
            any::<u32>().prop_map(|b| Command::OfferForAuction {
                item: AuctionItem::Land(BlockId(b))
            }),
            (any::<u32>(), "[a-z]{1,10}", any::<[u32; 5]>()).prop_map(|(b, type_id, n)| {
                Command::Build {
                    block: BlockId(b),
                    type_id,
                    footprint: Footprint {
                        x: n[0],
                        y: n[1],
                        w: n[2],
                        d: n[3],
                    },
                    height: n[4],
                    fit_out: FitOutLevel::High,
                }
            }),
            any::<bool>().prop_map(|insured| Command::SetInsurance { insured }),
        ]
    }

    fn seat() -> impl Strategy<Value = Seat> {
        (
            any::<bool>(),
            "[a-z:]{0,16}",
            "[0-9a-f]{0,16}",
            proptest::option::of("anon-[0-9a-f]{4}"),
        )
            .prop_map(|(human, brain, brain_version, player_id)| Seat {
                kind: if human { SeatKind::Human } else { SeatKind::Ai },
                brain,
                brain_version,
                player_id,
            })
    }

    prop_compose! {
        fn record()(dt in 0u64..20_000, seat in any::<u8>(), core in any::<bool>(),
                    accepted in any::<bool>(), command in command()) -> (u64, CommandRecord) {
            (dt, CommandRecord {
                tick: 0,
                seat,
                origin: if core { Origin::Core } else { Origin::External },
                accepted,
                command,
            })
        }
    }

    prop_compose! {
        fn checkpoint()(day in any::<u32>(), tick in any::<u64>(), state_hash in any::<u64>(),
                        seats in proptest::collection::vec(
                            (any::<i64>(), any::<i64>(), any::<u32>(), any::<u32>(), any::<bool>()),
                            0..6)) -> Checkpoint {
            Checkpoint {
                day,
                tick,
                state_hash,
                seats: seats
                    .into_iter()
                    .map(|(net_worth, cash, leases, businesses, bankrupt)| SeatCheckpoint {
                        net_worth, cash, leases, businesses, bankrupt,
                    })
                    .collect(),
            }
        }
    }

    prop_compose! {
        pub(crate) fn replay()(
            ids in ("[a-z0-9-]{0,20}", "[a-z0-9-]{0,20}", "[0-9a-f]{0,40}"),
            hashes in ("[0-9a-f]{16}", "[0-9a-f]{16}"),
            seed in any::<u64>(),
            years in (any::<i32>(), any::<i32>()),
            seats in proptest::collection::vec(seat(), 0..6),
            ai_order in proptest::collection::vec(any::<u8>(), 0..6),
            records in proptest::collection::vec(record(), 0..200),
            checkpoints in proptest::collection::vec(checkpoint(), 0..30),
            winner in prop_oneof![any::<u8>().prop_map(Winner::Seat), Just(Winner::Tie), Just(Winner::Nobody)],
            outcome_seats in proptest::collection::vec((any::<u8>(), any::<i64>(), any::<bool>()), 0..6),
            end in (any::<u32>(), any::<u64>(), any::<u64>()),
        ) -> Replay {
            let mut tick = 0u64;
            let commands = records
                .into_iter()
                .map(|(dt, mut r)| {
                    tick += dt;
                    r.tick = tick;
                    r
                })
                .collect();
            Replay {
                header: Header {
                    game_id: ids.0,
                    run_id: ids.1,
                    git_commit: ids.2,
                    rules_fingerprint: hashes.0,
                    config_hash: hashes.1,
                    seed,
                    preset: "v1".into(),
                    start_year: years.0,
                    end_year: years.1,
                    seats,
                    ai_order,
                },
                commands,
                checkpoints,
                outcome: Outcome {
                    winner,
                    seats: outcome_seats
                        .into_iter()
                        .map(|(rank, net_worth, bankrupt)| SeatOutcome { rank, net_worth, bankrupt })
                        .collect(),
                    days_played: end.0,
                    ticks: end.1,
                    final_state_hash: end.2,
                },
            }
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        /// Encoding round-trips: decode(encode(r)) == r, and the envelope says how long it is.
        #[test]
        fn encoding_round_trips(r in replay()) {
            let bytes = encode(&r).unwrap();
            let (back, used) = decode_one(&bytes).unwrap();
            prop_assert_eq!(used, bytes.len());
            prop_assert_eq!(back, r);
        }

        /// Envelopes back to back (a segment) decode one by one, with their offsets.
        #[test]
        fn segments_round_trip(a in replay(), b in replay()) {
            let (ea, eb) = (encode(&a).unwrap(), encode(&b).unwrap());
            let segment = [ea.clone(), eb.clone()].concat();
            let all = decode_all(&segment).unwrap();
            prop_assert_eq!(all.len(), 2);
            prop_assert_eq!((all[0].1, all[0].2), (0, ea.len()));
            prop_assert_eq!((all[1].1, all[1].2), (ea.len(), eb.len()));
            prop_assert_eq!(&all[0].0, &a);
            prop_assert_eq!(&all[1].0, &b);
        }
    }

    fn sample() -> Replay {
        use proptest::strategy::ValueTree;
        let mut runner = proptest::test_runner::TestRunner::deterministic();
        replay().new_tree(&mut runner).unwrap().current()
    }

    #[test]
    fn the_envelope_header_is_as_documented() {
        let bytes = encode(&sample()).unwrap();
        assert_eq!(&bytes[0..4], b"OTRP");
        assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]), FORMAT_VERSION);
        assert_eq!(u16::from_le_bytes([bytes[6], bytes[7]]), FLAG_ZSTD);
        let len = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
        assert_eq!(bytes.len(), ENVELOPE_LEN + len);
        let crc = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
        assert_eq!(crc, crc32fast::hash(&bytes[ENVELOPE_LEN..]));
    }

    #[test]
    fn bad_envelopes_are_refused_by_name() {
        let good = encode(&sample()).unwrap();
        let mut magic = good.clone();
        magic[0] = b'X';
        assert_eq!(decode_one(&magic).unwrap_err(), DecodeError::Magic);
        let mut version = good.clone();
        version[4] = 9;
        assert_eq!(
            decode_one(&version).unwrap_err(),
            DecodeError::Version { found: 9 }
        );
        let mut corrupt = good.clone();
        let last = corrupt.len() - 1;
        corrupt[last] ^= 0xff;
        assert_eq!(decode_one(&corrupt).unwrap_err(), DecodeError::Checksum);
        assert!(matches!(
            decode_one(&good[..good.len() - 1]).unwrap_err(),
            DecodeError::Truncated { .. }
        ));
    }
}
