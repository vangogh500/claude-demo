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
    ItemAdded { item_id: ItemId, title: String },
    ItemActivated { item_id: ItemId },
}

#[derive(Debug, PartialEq)]
pub struct Snapshot {
    pub join_code: String,
    pub you: ParticipantId,
    pub facilitator_id: ParticipantId,
    pub participants: Vec<Participant>,
    pub items: Vec<ItemView>,
    pub active_item_id: Option<ItemId>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ItemState {
    Pending,
    Voting,
    Revealed,
    Estimated,
}

/// An Item as a snapshot shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemView {
    pub id: ItemId,
    pub title: String,
    pub state: ItemState,
    pub result: Option<String>,
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
    NotFacilitator,
    UnknownItem,
    ItemAlreadyEstimated,
}

/// What an accepted command did.
#[derive(Debug, Default, PartialEq)]
pub struct Outcome {
    /// The Participant the sender became, when the command admitted it.
    pub admitted: Option<ParticipantId>,
    /// Events for the sender alone.
    pub reply: Vec<Event>,
    /// Events for every Participant in the Room, the sender included.
    pub broadcast: Vec<Event>,
}

pub struct Room {
    join_code: String,
    participants: Vec<Participant>,
    facilitator_id: ParticipantId,
    next_participant_id: ParticipantId,
    items: Vec<Item>,
    next_item_id: ItemId,
    active_item_id: Option<ItemId>,
}

struct Item {
    id: ItemId,
    title: String,
    state: ItemState,
    result: Option<String>,
}

impl Room {
    pub fn new(join_code: String) -> Room {
        Room {
            join_code,
            participants: Vec::new(),
            facilitator_id: 0,
            next_participant_id: 1,
            items: Vec::new(),
            next_item_id: 1,
            active_item_id: None,
        }
    }

    /// Applies `command` sent by `actor`.
    ///
    /// `create_room` from a Newcomer admits it as Participant 1 and Facilitator; the
    /// caller applies it once, to a Room fresh from [`Room::new`]. `create_room` or
    /// `join_room` from a Participant is refused with `already_in_room`. Every other
    /// command is not modelled yet: it is accepted with an empty [`Outcome`] and
    /// leaves the Room unchanged. Which commands a Newcomer may send at all is the
    /// socket layer's first-message rule, not the Room's.
    pub fn apply(&mut self, actor: Actor, command: Command) -> Result<Outcome, ErrorCode> {
        match (actor, command) {
            (Actor::Newcomer, Command::CreateRoom { name }) => Ok(self.create(name)),
            (Actor::Participant(_), Command::CreateRoom { .. } | Command::JoinRoom { .. }) => {
                Err(ErrorCode::AlreadyInRoom)
            }
            (Actor::Participant(me), Command::AddItem { .. } | Command::ActivateItem { .. })
                if me != self.facilitator_id =>
            {
                Err(ErrorCode::NotFacilitator)
            }
            (Actor::Participant(_), Command::AddItem { title }) => Ok(self.add_item(title)),
            (Actor::Participant(_), Command::ActivateItem { item_id }) => {
                self.activate_item(item_id)
            }
            _ => Ok(Outcome::default()),
        }
    }

    fn activate_item(&mut self, item_id: ItemId) -> Result<Outcome, ErrorCode> {
        let target = self
            .items
            .iter()
            .find(|item| item.id == item_id)
            .ok_or(ErrorCode::UnknownItem)?;
        if target.state == ItemState::Estimated {
            return Err(ErrorCode::ItemAlreadyEstimated);
        }
        // Skipping: the old Active Item goes back to Pending (its Votes, once they
        // exist, are discarded with it).
        if let Some(old) = self.active_item_id {
            self.item_mut(old).state = ItemState::Pending;
        }
        self.item_mut(item_id).state = ItemState::Voting;
        self.active_item_id = Some(item_id);
        Ok(Outcome {
            broadcast: vec![Event::ItemActivated { item_id }],
            ..Outcome::default()
        })
    }

    fn item_mut(&mut self, id: ItemId) -> &mut Item {
        self.items
            .iter_mut()
            .find(|item| item.id == id)
            .expect("item ids in the Room exist")
    }

    fn add_item(&mut self, title: String) -> Outcome {
        let id = self.next_item_id;
        self.next_item_id += 1;
        self.items.push(Item {
            id,
            title: title.clone(),
            state: ItemState::Pending,
            result: None,
        });
        Outcome {
            broadcast: vec![Event::ItemAdded { item_id: id, title }],
            ..Outcome::default()
        }
    }

    fn create(&mut self, name: String) -> Outcome {
        let id = self.admit(name);
        self.facilitator_id = id;
        Outcome {
            admitted: Some(id),
            broadcast: Vec::new(),
            reply: vec![Event::RoomSnapshot(self.snapshot_for(id))],
        }
    }

    fn admit(&mut self, name: String) -> ParticipantId {
        let id = self.next_participant_id;
        self.next_participant_id += 1;
        self.participants.push(Participant { id, name });
        id
    }

    pub fn snapshot_for(&self, you: ParticipantId) -> Snapshot {
        Snapshot {
            join_code: self.join_code.clone(),
            you,
            facilitator_id: self.facilitator_id,
            participants: self.participants.clone(),
            items: self
                .items
                .iter()
                .map(|item| ItemView {
                    id: item.id,
                    title: item.title.clone(),
                    state: item.state,
                    result: item.result.clone(),
                })
                .collect(),
            active_item_id: self.active_item_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room_with_facilitator() -> Room {
        let mut room = Room::new("ABCD".to_string());
        room.apply(
            Actor::Newcomer,
            Command::CreateRoom {
                name: "Ada".to_string(),
            },
        )
        .unwrap();
        room
    }

    fn add(room: &mut Room, title: &str) -> Result<Outcome, ErrorCode> {
        room.apply(
            Actor::Participant(1),
            Command::AddItem {
                title: title.to_string(),
            },
        )
    }

    #[test]
    fn add_item_broadcasts_item_added_with_increasing_ids() {
        let mut room = room_with_facilitator();

        let first = add(&mut room, "Login page").unwrap();
        let second = add(&mut room, "Logout page").unwrap();

        assert_eq!(
            first.broadcast,
            vec![Event::ItemAdded {
                item_id: 1,
                title: "Login page".to_string()
            }]
        );
        assert_eq!(
            second.broadcast,
            vec![Event::ItemAdded {
                item_id: 2,
                title: "Logout page".to_string()
            }]
        );
    }

    #[test]
    fn a_voter_cannot_add_an_item() {
        let mut room = room_with_facilitator();

        let refused = room.apply(
            Actor::Participant(2),
            Command::AddItem {
                title: "Nope".to_string(),
            },
        );

        assert_eq!(refused, Err(ErrorCode::NotFacilitator));
    }

    #[test]
    fn a_voter_cannot_activate_an_item() {
        let mut room = room_with_facilitator();
        add(&mut room, "Login page").unwrap();

        let refused = room.apply(Actor::Participant(2), Command::ActivateItem { item_id: 1 });

        assert_eq!(refused, Err(ErrorCode::NotFacilitator));
    }

    fn activate(room: &mut Room, item_id: ItemId) -> Result<Outcome, ErrorCode> {
        room.apply(Actor::Participant(1), Command::ActivateItem { item_id })
    }

    fn item_view(id: ItemId, title: &str, state: ItemState) -> ItemView {
        ItemView {
            id,
            title: title.to_string(),
            state,
            result: None,
        }
    }

    #[test]
    fn activating_a_pending_item_makes_it_the_voting_active_item() {
        let mut room = room_with_facilitator();
        add(&mut room, "Login page").unwrap();
        add(&mut room, "Logout page").unwrap();

        let outcome = activate(&mut room, 2).unwrap();

        assert_eq!(outcome.broadcast, vec![Event::ItemActivated { item_id: 2 }]);
        let snapshot = room.snapshot_for(1);
        assert_eq!(snapshot.active_item_id, Some(2));
        assert_eq!(
            snapshot.items,
            vec![
                item_view(1, "Login page", ItemState::Pending),
                item_view(2, "Logout page", ItemState::Voting),
            ]
        );
    }

    #[test]
    fn activating_another_item_sends_the_old_one_back_to_pending_under_one_event() {
        let mut room = room_with_facilitator();
        add(&mut room, "Login page").unwrap();
        add(&mut room, "Logout page").unwrap();
        activate(&mut room, 1).unwrap();

        let outcome = activate(&mut room, 2).unwrap();

        assert_eq!(outcome.broadcast, vec![Event::ItemActivated { item_id: 2 }]);
        let snapshot = room.snapshot_for(1);
        assert_eq!(snapshot.active_item_id, Some(2));
        assert_eq!(
            snapshot.items,
            vec![
                item_view(1, "Login page", ItemState::Pending),
                item_view(2, "Logout page", ItemState::Voting),
            ]
        );
    }

    #[test]
    fn activating_an_unknown_item_is_refused() {
        let mut room = room_with_facilitator();
        add(&mut room, "Login page").unwrap();

        assert_eq!(activate(&mut room, 9).unwrap_err(), ErrorCode::UnknownItem);
    }

    #[test]
    fn activating_an_estimated_item_is_refused() {
        let mut room = room_with_facilitator();
        add(&mut room, "Login page").unwrap();
        // `accept` is not modelled yet, so put the Item in Estimated directly.
        room.items[0].state = ItemState::Estimated;
        room.items[0].result = Some("5".to_string());

        assert_eq!(
            activate(&mut room, 1).unwrap_err(),
            ErrorCode::ItemAlreadyEstimated
        );
    }

    #[test]
    fn add_item_works_with_nobody_else_joined_and_items_keep_insertion_order() {
        let mut room = room_with_facilitator();
        add(&mut room, "B").unwrap();
        add(&mut room, "A").unwrap();

        assert_eq!(
            room.snapshot_for(1).items,
            vec![
                item_view(1, "B", ItemState::Pending),
                item_view(2, "A", ItemState::Pending)
            ]
        );
    }

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
                broadcast: vec![],
                reply: vec![Event::RoomSnapshot(Snapshot {
                    join_code: "ABCD".to_string(),
                    you: 1,
                    facilitator_id: 1,
                    participants: vec![Participant {
                        id: 1,
                        name: "Ada".to_string()
                    }],
                    items: vec![],
                    active_item_id: None,
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
}
