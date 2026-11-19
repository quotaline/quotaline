//! Answers the engine's callbacks for a terminal, which has no notifications or desktop clock preference.

use futures_util::future::BoxFuture;
use quotaline_engine::service::Host;

/// The terminal redraws from the engine's state on its own schedule, so change signals need no handling.
pub(crate) struct TerminalHost;

impl Host for TerminalHost {
    fn changed(&self, _state: String) -> BoxFuture<'_, ()> {
        Box::pin(async {})
    }

    fn log_changed(&self) -> BoxFuture<'_, ()> {
        Box::pin(async {})
    }

    fn notification_planned(&self) -> BoxFuture<'_, ()> {
        Box::pin(async {})
    }

    /// Retry times use the 24-hour clock, since a terminal reports no clock preference.
    fn twelve_hour_clock(&self) -> BoxFuture<'_, bool> {
        Box::pin(async { false })
    }
}

#[cfg(test)]
#[path = "../../tests/unit/desktop/host.rs"]
mod tests;
