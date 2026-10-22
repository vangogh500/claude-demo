//! The wire protocol: one JSON object per text frame, discriminated by `type`.
//!
//! Translates between frames and the Room's [`Command`], [`Event`] and [`ErrorCode`]
//! so the Room never sees JSON (ADR 0001).

use serde::{Deserialize, Serialize};

use crate::room::{Command, ErrorCode, Event, ItemId, ParticipantId};

/// A frame that is not JSON, has no known `type`, or lacks a field its `type` needs.
#[derive(Debug, PartialEq)]
pub struct Malformed;

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WireCommand {
    CreateRoom { name: String },
    JoinRoom { join_code: String, name: String },
    AddItem { title: String },
    ActivateItem { item_id: ItemId },
    Vote { value: String },
    Reveal,
    Revote,
    Accept { value: String },
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WireEvent<'a> {
    RoomSnapshot {
        join_code: &'a str,
        you: ParticipantId,
        facilitator_id: ParticipantId,
        participants: Vec<WireParticipant<'a>>,
        // Items, the Active Item and Votes are not modelled yet, so a snapshot
        // always carries them empty.
        items: [(); 0],
        active_item_id: Option<ItemId>,
        voted: [ParticipantId; 0],
    },
    Error {
        code: &'static str,
    },
}

#[derive(Serialize)]
struct WireParticipant<'a> {
    id: ParticipantId,
    name: &'a str,
}

pub fn decode(frame: &str) -> Result<Command, Malformed> {
    let command = serde_json::from_str(frame).map_err(|_| Malformed)?;
    Ok(match command {
        WireCommand::CreateRoom { name } => Command::CreateRoom { name },
        WireCommand::JoinRoom { join_code, name } => Command::JoinRoom { join_code, name },
        WireCommand::AddItem { title } => Command::AddItem { title },
        WireCommand::ActivateItem { item_id } => Command::ActivateItem { item_id },
        WireCommand::Vote { value } => Command::Vote { value },
        WireCommand::Reveal => Command::Reveal,
        WireCommand::Revote => Command::Revote,
        WireCommand::Accept { value } => Command::Accept { value },
    })
}

pub fn encode_event(event: &Event) -> String {
    let wire = match event {
        Event::RoomSnapshot(snapshot) => WireEvent::RoomSnapshot {
            join_code: &snapshot.join_code,
            you: snapshot.you,
            facilitator_id: snapshot.facilitator_id,
            participants: snapshot
                .participants
                .iter()
                .map(|p| WireParticipant {
                    id: p.id,
                    name: &p.name,
                })
                .collect(),
            items: [],
            active_item_id: None,
            voted: [],
        },
    };
    to_frame(&wire)
}

pub fn encode_error(code: ErrorCode) -> String {
    let code = match code {
        ErrorCode::NotInRoom => "not_in_room",
        ErrorCode::AlreadyInRoom => "already_in_room",
    };
    to_frame(&WireEvent::Error { code })
}

pub fn encode_malformed() -> String {
    to_frame(&WireEvent::Error { code: "malformed" })
}

fn to_frame(wire: &WireEvent) -> String {
    serde_json::to_string(wire).expect("wire events serialize to JSON")
}
