//! The Room: a transport-free state machine (ADR 0001).
//!
//! A Room knows nothing about sockets or JSON. [`Room::apply`] takes a command and
//! who sent it, and returns either an [`Outcome`] or an [`ErrorCode`].

/// A Participant's id: assigned by the Room from 1 upwards in joining order.
pub type ParticipantId = u32;
/// An Item's id: assigned by the Room in the order Items are added.
pub type ItemId = u32;

/// Everything a client can ask of a Room, as named in the spec's wire protocol.
#[derive(Debug)]
pub enum Command {
    CreateRoom { name: String },
    JoinRoom { join_code: String, name: String },
    AddItem { title: String },
    ActivateItem { item_id: ItemId },
    Vote { value: String },
    Reveal,
    Revote,
    Accept { value: String },
}

/// Who sends a command: someone not yet in the Room, or one of its Participants.
pub enum Actor {
    Newcomer,
    Participant(ParticipantId),
}

#[derive(Debug, PartialEq)]
pub enum Event {
    RoomSnapshot(Snapshot),
}

#[derive(Debug, PartialEq)]
pub struct Snapshot {
    pub join_code: String,
    pub you: ParticipantId,
    pub facilitator_id: ParticipantId,
    pub participants: Vec<Participant>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Participant {
    pub id: ParticipantId,
    pub name: String,
}

#[derive(Debug, PartialEq)]
pub enum ErrorCode {
    NotInRoom,
    AlreadyInRoom,
}

/// What an accepted command did.
#[derive(Debug, Default, PartialEq)]
pub struct Outcome {
    /// The Participant the sender became, when the command admitted it.
    pub admitted: Option<ParticipantId>,
    /// Events for the sender alone.
    pub reply: Vec<Event>,
}

pub struct Room {
    join_code: String,
    participants: Vec<Participant>,
    facilitator_id: ParticipantId,
    next_participant_id: ParticipantId,
}

impl Room {
    pub fn new(join_code: String) -> Room {
        Room {
            join_code,
            participants: Vec::new(),
            facilitator_id: 0,
            next_participant_id: 1,
        }
    }

    /// Applies `command` sent by `actor`.
    ///
    /// `create_room` from a Newcomer admits it as Participant 1 and Facilitator; the
    /// caller applies it once, to a Room fresh from [`Room::new`]. `create_room` or
    /// `join_room` from a Participant is refused with `already_in_room`, and any
    /// other command from a Newcomer with `not_in_room`. Joining and every Item and
    /// Vote command are not modelled yet: they are accepted with an empty
    /// [`Outcome`] and leave the Room unchanged.
    pub fn apply(&mut self, actor: Actor, command: Command) -> Result<Outcome, ErrorCode> {
        match (actor, command) {
            (Actor::Newcomer, Command::CreateRoom { name }) => Ok(self.create(name)),
            (Actor::Newcomer, Command::JoinRoom { .. }) => Ok(Outcome::default()),
            (Actor::Newcomer, _) => Err(ErrorCode::NotInRoom),
            (Actor::Participant(_), Command::CreateRoom { .. } | Command::JoinRoom { .. }) => {
                Err(ErrorCode::AlreadyInRoom)
            }
            (Actor::Participant(_), _) => Ok(Outcome::default()),
        }
    }

    fn create(&mut self, name: String) -> Outcome {
        let id = self.admit(name);
        self.facilitator_id = id;
        Outcome {
            admitted: Some(id),
            reply: vec![Event::RoomSnapshot(self.snapshot_for(id))],
        }
    }

    fn admit(&mut self, name: String) -> ParticipantId {
        let id = self.next_participant_id;
        self.next_participant_id += 1;
        self.participants.push(Participant { id, name });
        id
    }

    fn snapshot_for(&self, you: ParticipantId) -> Snapshot {
        Snapshot {
            join_code: self.join_code.clone(),
            you,
            facilitator_id: self.facilitator_id,
            participants: self.participants.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_room_makes_the_creator_the_facilitator_and_only_participant() {
        let mut room = Room::new("ABCD".to_string());

        let outcome = room
            .apply(
                Actor::Newcomer,
                Command::CreateRoom {
                    name: "Ada".to_string(),
                },
            )
            .unwrap();

        assert_eq!(
            outcome,
            Outcome {
                admitted: Some(1),
                reply: vec![Event::RoomSnapshot(Snapshot {
                    join_code: "ABCD".to_string(),
                    you: 1,
                    facilitator_id: 1,
                    participants: vec![Participant {
                        id: 1,
                        name: "Ada".to_string()
                    }],
                })],
            }
        );
    }

    #[test]
    fn a_participant_cannot_create_a_room_again() {
        let mut room = Room::new("ABCD".to_string());
        room.apply(
            Actor::Newcomer,
            Command::CreateRoom {
                name: "Ada".to_string(),
            },
        )
        .unwrap();

        let refused = room.apply(
            Actor::Participant(1),
            Command::CreateRoom {
                name: "Ada again".to_string(),
            },
        );

        assert_eq!(refused, Err(ErrorCode::AlreadyInRoom));
    }

    #[test]
    fn a_participant_cannot_join_a_room_again() {
        let mut room = Room::new("ABCD".to_string());
        room.apply(
            Actor::Newcomer,
            Command::CreateRoom {
                name: "Ada".to_string(),
            },
        )
        .unwrap();

        let refused = room.apply(
            Actor::Participant(1),
            Command::JoinRoom {
                join_code: "ABCD".to_string(),
                name: "Ada".to_string(),
            },
        );

        assert_eq!(refused, Err(ErrorCode::AlreadyInRoom));
    }

    #[test]
    fn a_newcomer_must_create_or_join_before_anything_else() {
        let mut room = Room::new("ABCD".to_string());
        room.apply(
            Actor::Newcomer,
            Command::CreateRoom {
                name: "Ada".to_string(),
            },
        )
        .unwrap();

        let refused = room.apply(
            Actor::Newcomer,
            Command::AddItem {
                title: "Login".to_string(),
            },
        );

        assert_eq!(refused, Err(ErrorCode::NotInRoom));
    }
}
