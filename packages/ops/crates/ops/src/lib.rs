//! Ops: the LCARS daemon.
//!
//! One Herdr socket connection per host, merged into a single normalised
//! session list, served to the Stream Deck plugin over loopback HTTP and
//! WebSocket. See `docs/HANDOFF.md` and `docs/herdr-api.md` in the repo.

pub mod actions;
pub mod config;
pub mod herdr;
pub mod model;
pub mod server;

/// Herdr private protocol this build speaks. Checked against every host's
/// `pong` on connect; a mismatch is a hard error for that host.
pub const HERDR_PROTOCOL: u32 = 22;
