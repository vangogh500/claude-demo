//! The wire protocol: one JSON object per text frame, discriminated by `type`.
//!
//! Translates between frames and the Room's [`Command`], [`Event`] and [`ErrorCode`]
//! so the Room never sees JSON (ADR 0001).

use serde::{Deserialize, Serialize};

use crate::room::{Command, ErrorCode, Event, ItemId, ItemState, ItemView, ParticipantId};

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
        items: Vec<WireItem<'a>>,
        active_item_id: Option<ItemId>,
        // Votes are not modelled yet, so a snapshot always carries none.
        voted: [ParticipantId; 0],
    },
    ParticipantJoined {
        participant_id: ParticipantId,
        name: &'a str,
    },
    ParticipantLeft {
        participant_id: ParticipantId,
    },
    FacilitatorChanged {
        participant_id: ParticipantId,
    },
    ItemActivated {
        item_id: ItemId,
    },
    ItemAdded {
        item_id: ItemId,
        title: &'a str,
    },
    Error {
        code: &'static str,
    },
}

#[derive(Serialize)]
struct WireItem<'a> {
    id: ItemId,
    title: &'a str,
    state: &'static str,
    result: Option<&'a str>,
}

impl<'a> From<&'a ItemView> for WireItem<'a> {
    fn from(item: &'a ItemView) -> Self {
        WireItem {
            id: item.id,
            title: &item.title,
            state: match item.state {
                ItemState::Pending => "Pending",
                ItemState::Voting => "Voting",
                ItemState::Revealed => "Revealed",
                ItemState::Estimated => "Estimated",
            },
            result: item.result.as_deref(),
        }
    }
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
            items: snapshot.items.iter().map(WireItem::from).collect(),
            active_item_id: snapshot.active_item_id,
            voted: [],
        },
        Event::ParticipantJoined {
            participant_id,
            name,
        } => WireEvent::ParticipantJoined {
            participant_id: *participant_id,
            name,
        },
        Event::ParticipantLeft { participant_id } => WireEvent::ParticipantLeft {
            participant_id: *participant_id,
        },
        Event::FacilitatorChanged { participant_id } => WireEvent::FacilitatorChanged {
            participant_id: *participant_id,
        },
        Event::ItemActivated { item_id } => WireEvent::ItemActivated { item_id: *item_id },
        Event::ItemAdded { item_id, title } => WireEvent::ItemAdded {
            item_id: *item_id,
            title,
        },
    };
    to_frame(&wire)
}

pub fn encode_error(code: ErrorCode) -> String {
    let code = match code {
        ErrorCode::NotInRoom => "not_in_room",
        ErrorCode::AlreadyInRoom => "already_in_room",
        ErrorCode::UnknownJoinCode => "unknown_join_code",
        ErrorCode::NotFacilitator => "not_facilitator",
        ErrorCode::UnknownItem => "unknown_item",
        ErrorCode::ItemAlreadyEstimated => "item_already_estimated",
    };
    to_frame(&WireEvent::Error { code })
}

pub fn encode_malformed() -> String {
    to_frame(&WireEvent::Error { code: "malformed" })
}

fn to_frame(wire: &WireEvent) -> String {
    serde_json::to_string(wire).expect("wire events serialize to JSON")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::room::{Participant, Snapshot};
    use serde_json::{Value, json};

    // No socket can reach an Estimated Item until `accept` is modelled, so the wire
    // shape of every Item state is pinned here, at the encoder, until it is.
    #[test]
    fn a_snapshot_with_items_encodes_state_strings_result_and_active_item_id() {
        let snapshot = Snapshot {
            join_code: "ABCD".to_string(),
            you: 1,
            facilitator_id: 1,
            participants: vec![Participant {
                id: 1,
                name: "Ada".to_string(),
            }],
            items: vec![
                ItemView {
                    id: 1,
                    title: "Login page".to_string(),
                    state: ItemState::Estimated,
                    result: Some("5".to_string()),
                },
                ItemView {
                    id: 2,
                    title: "Logout page".to_string(),
                    state: ItemState::Voting,
                    result: None,
                },
                ItemView {
                    id: 3,
                    title: "Profile".to_string(),
                    state: ItemState::Pending,
                    result: None,
                },
            ],
            active_item_id: Some(2),
        };

        let frame: Value =
            serde_json::from_str(&encode_event(&Event::RoomSnapshot(snapshot))).unwrap();

        assert_eq!(
            frame["items"],
            json!([
                {"id": 1, "title": "Login page", "state": "Estimated", "result": "5"},
                {"id": 2, "title": "Logout page", "state": "Voting", "result": null},
                {"id": 3, "title": "Profile", "state": "Pending", "result": null},
            ])
        );
        assert_eq!(frame["active_item_id"], 2);
    }
}
