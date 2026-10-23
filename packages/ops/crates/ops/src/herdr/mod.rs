//! Herdr socket client: wire framing, transports, and the per-session
//! connection loop that turns Herdr frames into `HostEvent`s.

pub mod client;
pub mod transport;
pub mod wire;
