//! Exposes GNOME credential, provider, logging, and storage adapters.

pub mod clock;
mod credentials;
pub mod history;
pub mod history_view;
pub mod logging;
pub mod providers;
pub mod state;
mod status_projection;
pub mod store;

pub use providers::claude_transcripts;
