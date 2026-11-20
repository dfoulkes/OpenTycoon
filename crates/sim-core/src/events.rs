//! Events: the newspaper, historical events, random events, elections (§9; R12, R16).
//!
//! Owner boundary: deciding which events fire on a year step and turning them into effects
//! other modules apply. Post-v1 except the headline plumbing.
//!
//! Status: step selection is real; effects land post-v1.

use serde::{Deserialize, Serialize};

use crate::config::events::{EventsConfig, HistoricalEvent};

/// A front page shown on the year step.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct FrontPage {
    /// Year of the step.
    pub year: i32,
    /// Headlines, in config order.
    pub headlines: Vec<String>,
}

/// Historical events that fire on the step starting at `step_year` and covering
/// `years_per_step` years: `step_year ≤ year < step_year + years_per_step` (§9).
pub fn events_for_step(
    config: &EventsConfig,
    step_year: i32,
    years_per_step: u32,
) -> Vec<&HistoricalEvent> {
    let end = i64::from(step_year) + i64::from(years_per_step);
    config
        .historical
        .iter()
        .filter(|e| e.year >= step_year && i64::from(e.year) < end)
        .collect()
}

/// Event state carried between steps.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Events {
    /// The latest front page.
    pub front_page: FrontPage,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::events::{ElectionSettings, HistoricalEvent};

    fn event(id: &str, year: i32) -> HistoricalEvent {
        HistoricalEvent {
            id: id.into(),
            year,
            headline: id.into(),
            effects: vec![],
        }
    }

    #[test]
    fn each_event_fires_on_the_step_covering_its_year() {
        let config = EventsConfig {
            historical: vec![
                event("blitz", 1940),
                event("decimal", 1971),
                event("crash", 2008),
            ],
            random: vec![],
            elections: ElectionSettings {
                enabled: false,
                weights: std::collections::BTreeMap::default(),
            },
        };
        let ids = |y| {
            events_for_step(&config, y, 5)
                .iter()
                .map(|e| e.id.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(1940), ["blitz"]);
        assert_eq!(ids(1970), ["decimal"]);
        assert_eq!(ids(2005), ["crash"]);
        assert_eq!(ids(1935), Vec::<String>::new());
    }
}
