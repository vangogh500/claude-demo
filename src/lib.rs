//! Story pointing server: Rooms where Participants Vote on Items over websockets.
//!
//! Two layers, per ADR 0001: [`room`] is a transport-free state machine, and
//! [`server`] decodes websocket frames with [`protocol`], routes them to a Room held
//! in the [`registry`], and writes the Room's events back as frames.

pub mod cli;
pub mod protocol;
pub mod registry;
pub mod room;
pub mod server;
