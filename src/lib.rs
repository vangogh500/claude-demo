//! Story pointing server: Rooms where Participants Vote on Items over websockets.
//!
//! Two layers, per ADR 0001: [`room`] is a transport-free state machine, and
//! [`server`] decodes websocket frames with [`protocol`], routes them to a Room held
//! in the [`registry`], and writes the Room's events back as frames. The same server
//! also serves the Web Client, one embedded page, at `GET /` (ADR 0002).

pub mod cli;
pub mod protocol;
pub mod registry;
pub mod room;
pub mod server;
