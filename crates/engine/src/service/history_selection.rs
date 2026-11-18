//! Adapts history selection events to the shared policy.

use super::Error;
use quotaline_analytics as analytics;

pub fn step(count: i32, selected: i32, directions: &[i32]) -> Result<i32, Error> {
    if count < 0 || selected < -1 || selected >= count {
        return Err(Error::InvalidArgs("Invalid history reading index".into()));
    }
    Ok(
        analytics::step_reading(count as usize, (selected >= 0).then_some(selected as usize), directions)
            .map_or(-1, |index| index as i32),
    )
}

#[cfg(test)]
#[path = "../../tests/unit/service/history_selection/tests.rs"]
mod tests;
