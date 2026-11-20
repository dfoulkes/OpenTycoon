//! Steps for `features/clock/`.

use cucumber::{then, when};
use sim_core::clock::{Clock, ClockEvents, ClockRules, TradingWindow};

use crate::SimWorld;

/// The clock under test and what has fired since the last When step.
#[derive(Debug, Default)]
pub struct ClockState {
    clock: Option<Clock>,
    fired: ClockEvents,
}

impl ClockState {
    /// Forgets the clock so it is rebuilt from the (possibly edited) config.
    pub fn reset(&mut self) {
        self.clock = None;
        self.fired = ClockEvents::default();
    }
}

fn clock(w: &mut SimWorld) -> &mut Clock {
    if w.clock.clock.is_none() {
        let config = w.config.load_ok();
        w.clock.clock = Some(Clock::new(ClockRules::from_config(&config.game.clock)));
    }
    w.clock.clock.as_mut().unwrap()
}

fn advance(w: &mut SimWorld, ticks: u64) {
    let mut fired = ClockEvents::default();
    let c = clock(w);
    for _ in 0..ticks {
        let e = c.advance();
        fired.midnight |= e.midnight;
        fired.arrivals |= e.arrivals;
        fired.settlement |= e.settlement;
        fired.game_over |= e.game_over;
    }
    w.clock.fired = fired;
}

fn parse_hms(text: &str) -> u32 {
    let parts: Vec<u32> = text.split(':').map(|p| p.parse().unwrap()).collect();
    parts[0] * 3600 + parts[1] * 60 + parts.get(2).copied().unwrap_or(0)
}

#[when(expr = "the clock runs to {string}")]
fn runs_to(w: &mut SimWorld, time: String) {
    let target = parse_hms(&time);
    let (step, per_day) = {
        let rules = clock(w).rules();
        (
            u64::from(rules.tick_game_seconds),
            u64::from(rules.ticks_per_day()),
        )
    };
    assert_eq!(u64::from(target) % step, 0, "{time} is not on a tick");
    let now = u64::from(clock(w).second_of_day()) / step;
    let goal = u64::from(target) / step;
    let ticks = if goal > now {
        goal - now
    } else {
        per_day - now + goal
    };
    advance(w, ticks);
}

#[when(regex = r"^the clock advances (\d+) ticks?$")]
fn advances(w: &mut SimWorld, ticks: u64) {
    advance(w, ticks);
}

#[when(regex = r"^the clock runs for (\d+) days?$")]
fn runs_days(w: &mut SimWorld, days: u64) {
    let per_day = u64::from(clock(w).rules().ticks_per_day());
    advance(w, days * per_day);
}

#[then(expr = "a day is {int} ticks")]
fn day_ticks(w: &mut SimWorld, ticks: u32) {
    assert_eq!(clock(w).rules().ticks_per_day(), ticks);
}

#[then(expr = "a day lasts {int} real seconds")]
fn day_real_seconds(w: &mut SimWorld, seconds: u64) {
    assert_eq!(clock(w).rules().real_millis_per_day(), seconds * 1000);
}

#[then(expr = "a full game is {int} days")]
fn full_game(w: &mut SimWorld, days: u32) {
    assert_eq!(clock(w).rules().total_days(), days);
}

#[then(expr = "the year is {int}")]
fn year_is(w: &mut SimWorld, year: i32) {
    assert_eq!(clock(w).year(), year);
}

#[then(expr = "the time is {string}")]
fn time_is(w: &mut SimWorld, time: String) {
    assert_eq!(clock(w).second_of_day(), parse_hms(&time));
}

#[then(regex = r"^the (midnight reckoning|arrivals|settlement) ha(?:s|ve) (fired|not fired)$")]
fn fired(w: &mut SimWorld, what: String, state: String) {
    let f = w.clock.fired;
    let actual = match what.as_str() {
        "midnight reckoning" => f.midnight,
        "arrivals" => f.arrivals,
        _ => f.settlement,
    };
    assert_eq!(actual, state == "fired", "{what} expected {state}");
}

#[then(regex = r"^the game is (over|not over)$")]
fn game_over(w: &mut SimWorld, state: String) {
    let over = clock(w).is_over();
    assert_eq!(over, state == "over");
    if over {
        assert!(
            w.clock.fired.game_over,
            "the final midnight should report game_over"
        );
    }
}

#[then(regex = r"^(day trade|night trade|hotel rooms) (?:is|are) (open|closed)$")]
fn window(w: &mut SimWorld, which: String, state: String) {
    let window = match which.as_str() {
        "day trade" => TradingWindow::Day,
        "night trade" => TradingWindow::Night,
        _ => TradingWindow::Hotel,
    };
    let clock = clock(w);
    let (hour, minute, second) = clock.time_of_day();
    assert_eq!(
        clock.is_open(window),
        state == "open",
        "{which} at {hour:02}:{minute:02}:{second:02}"
    );
}
