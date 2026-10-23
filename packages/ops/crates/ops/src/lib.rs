//! Ops: the LCARS daemon.
//!
//! One Herdr socket connection per Herdr session, merged into a single normalised
//! fleet of sessions, workspaces and panes, served to the Stream Deck plugin over loopback HTTP and
//! WebSocket. See `docs/HANDOFF.md` and `docs/herdr-api.md` in the repo.

pub mod actions;
pub mod banners;
pub mod config;
pub mod herdr;
pub mod model;
pub mod reveal;
pub mod server;

/// Herdr private protocol this build speaks. Checked against every session's
/// `pong` on connect; a mismatch is a hard error for that session.
pub const HERDR_PROTOCOL: u32 = 22;
