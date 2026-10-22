//! The Room: a transport-free state machine (ADR 0001).
//!
//! A Room knows nothing about sockets or JSON. [`Room::apply`] takes a command and
//! who sent it, and returns either an [`Outcome`] or an [`ErrorCode`].

/// A Participant's id: assigned by the Room from 1 upwards in joining order.
pub type ParticipantId = u32;
/// An Item's id: assigned by the Room in the order Items are added.
pub type ItemId = u32;

/// Everything that can be applied to a Room: the spec's eight wire commands, plus
/// [`Command::Leave`], which no client sends.
#[derive(Debug)]
pub enum Command {
    CreateRoom {
        name: String,
    },
    JoinRoom {
        join_code: String,
        name: String,
    },
    AddItem {
        title: String,
    },
    ActivateItem {
        item_id: ItemId,
    },
    Vote {
        value: String,
    },
    Reveal,
    Revote,
    Accept {
        value: String,
    },
    /// The sender has gone: the socket layer applies this when a bound socket
    /// closes. It is not part of the wire protocol.
    Leave,
}

/// Who sends a command: someone not yet in the Room, or one of its Participants.
pub enum Actor {
    Newcomer,
    Participant(ParticipantId),
}

#[derive(Debug, PartialEq)]
pub enum Event {
    RoomSnapshot(Snapshot),
    ParticipantJoined {
        participant_id: ParticipantId,
        name: String,
    },
    ParticipantLeft {
        participant_id: ParticipantId,
    },
    FacilitatorChanged {
        participant_id: ParticipantId,
    },
    ItemAdded {
        item_id: ItemId,
        title: String,
    },
    ItemActivated {
        item_id: ItemId,
    },
    /// Names who voted, never what.
    VoteCast {
        participant_id: ParticipantId,
    },
    VotesRevealed {
        item_id: ItemId,
        /// Every Vote as (participant, value), in the order first cast. The wire map
        /// built from it carries no order.
        votes: Vec<(ParticipantId, String)>,
    },
    VotesCleared {
        item_id: ItemId,
    },
    ItemEstimated {
        item_id: ItemId,
        result: String,
    },
}

/// The fixed Scale a Vote may take.
const SCALE: [&str; 11] = ["0", "1", "2", "3", "5", "8", "13", "20", "40", "100", "?"];

#[derive(Debug, PartialEq)]
pub struct Snapshot {
    pub join_code: String,
    pub you: ParticipantId,
    pub facilitator_id: ParticipantId,
    pub participants: Vec<Participant>,
    pub items: Vec<ItemView>,
    pub active_item_id: Option<ItemId>,
    /// Participants with a Vote on the Active Item.
    pub voted: Vec<ParticipantId>,
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
    UnknownJoinCode,
    NotFacilitator,
    UnknownItem,
    ItemAlreadyEstimated,
    NoActiveItem,
    ItemNotVoting,
    ItemNotRevealed,
    InvalidValue,
    NoVotes,
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
    /// The command left the Room with no Participants: the Room has ended, and its
    /// owner should drop it and release its Join Code.
    pub ended: bool,
}

pub struct Room {
    join_code: String,
    participants: Vec<Participant>,
    facilitator_id: ParticipantId,
    next_participant_id: ParticipantId,
    items: Vec<ItemView>,
    next_item_id: ItemId,
    active_item_id: Option<ItemId>,
    /// Votes on the Active Item, in the order first cast.
    votes: Vec<(ParticipantId, String)>,
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
            votes: Vec::new(),
        }
    }

    /// Applies `command` sent by `actor`.
    ///
    /// `create_room` from a Newcomer admits it as Participant 1 and Facilitator; the caller
    /// applies it once, to a Room fresh from [`Room::new`]. `join_room` from a Newcomer
    /// admits it as a Voter, replying with a snapshot and broadcasting `participant_joined`;
    /// its Join Code is not checked here, since whoever routed the command to this Room has
    /// already resolved it. `leave` from a Participant removes it and broadcasts
    /// `participant_left`; if it was the Facilitator, the remaining Participant with the
    /// lowest id is promoted (`facilitator_changed`), and if nobody remains the [`Outcome`]
    /// reports the Room ended. `leave` from anyone not in the Room changes nothing.
    /// `create_room` or `join_room` from a Participant is refused with `already_in_room`.
    /// `add_item` and `activate_item` are the Facilitator's: a Voter gets `not_facilitator`.
    /// `add_item` appends a Pending Item. `activate_item` makes a Pending Item the Voting
    /// Active Item (sending any previous Active Item back to Pending); it refuses an unknown
    /// id (`unknown_item`) or an Estimated Item (`item_already_estimated`), and is a no-op on
    /// the Item already Active. `vote` (any Participant) records or replaces the sender's
    /// Vote on a Voting Active Item and broadcasts `vote_cast`; it refuses a value off the
    /// Scale (`invalid_value`), no Active Item (`no_active_item`) and a Revealed Item
    /// (`item_not_voting`). `reveal` (Facilitator) makes the Voting Active Item Revealed and
    /// broadcasts `votes_revealed`; it refuses no Active Item (`no_active_item`), an Item
    /// already Revealed (`item_not_voting`) and an Item with no Votes (`no_votes`). `revote`
    /// (Facilitator) clears the Votes, returns the Revealed Active Item to Voting and
    /// broadcasts `votes_cleared`. `accept` (Facilitator) records a Scale value (`invalid_value`
    /// otherwise; it need not match any Vote) as the Revealed Active Item's Result, makes it
    /// Estimated, leaves no Active Item and broadcasts `item_estimated`. Both refuse no Active
    /// Item (`no_active_item`) and an Item not Revealed (`item_not_revealed`). Which commands a
    /// Newcomer may send at all is the socket layer's first-message rule, not the Room's.
    pub fn apply(&mut self, actor: Actor, command: Command) -> Result<Outcome, ErrorCode> {
        match (actor, command) {
            (Actor::Newcomer, Command::CreateRoom { name }) => Ok(self.create(name)),
            (Actor::Newcomer, Command::JoinRoom { name, .. }) => Ok(self.join(name)),
            (Actor::Participant(me), Command::Leave) => Ok(self.leave(me)),
            (Actor::Participant(_), Command::CreateRoom { .. } | Command::JoinRoom { .. }) => {
                Err(ErrorCode::AlreadyInRoom)
            }
            (
                Actor::Participant(me),
                Command::AddItem { .. }
                | Command::ActivateItem { .. }
                | Command::Reveal
                | Command::Revote
                | Command::Accept { .. },
            ) if me != self.facilitator_id => Err(ErrorCode::NotFacilitator),
            (Actor::Participant(_), Command::AddItem { title }) => Ok(self.add_item(title)),
            (Actor::Participant(_), Command::ActivateItem { item_id }) => {
                self.activate_item(item_id)
            }
            (Actor::Participant(me), Command::Vote { value }) => self.vote(me, value),
            (Actor::Participant(_), Command::Reveal) => self.reveal(),
            (Actor::Participant(_), Command::Revote) => self.revote(),
            (Actor::Participant(_), Command::Accept { value }) => self.accept(value),
            (Actor::Newcomer, _) => Ok(Outcome::default()),
        }
    }

    fn revealed_item_mut(&mut self) -> Result<&mut ItemView, ErrorCode> {
        let item = self.active_item_mut().ok_or(ErrorCode::NoActiveItem)?;
        if item.state != ItemState::Revealed {
            return Err(ErrorCode::ItemNotRevealed);
        }
        Ok(item)
    }

    fn revote(&mut self) -> Result<Outcome, ErrorCode> {
        let item = self.revealed_item_mut()?;
        item.state = ItemState::Voting;
        let item_id = item.id;
        self.votes.clear();
        Ok(Outcome {
            broadcast: vec![Event::VotesCleared { item_id }],
            ..Outcome::default()
        })
    }

    fn accept(&mut self, value: String) -> Result<Outcome, ErrorCode> {
        if !SCALE.contains(&value.as_str()) {
            return Err(ErrorCode::InvalidValue);
        }
        let item = self.revealed_item_mut()?;
        item.state = ItemState::Estimated;
        item.result = Some(value.clone());
        let item_id = item.id;
        self.active_item_id = None;
        self.votes.clear();
        Ok(Outcome {
            broadcast: vec![Event::ItemEstimated {
                item_id,
                result: value,
            }],
            ..Outcome::default()
        })
    }

    fn active_item_mut(&mut self) -> Option<&mut ItemView> {
        let id = self.active_item_id?;
        self.items.iter_mut().find(|i| i.id == id)
    }

    fn vote(&mut self, me: ParticipantId, value: String) -> Result<Outcome, ErrorCode> {
        if !SCALE.contains(&value.as_str()) {
            return Err(ErrorCode::InvalidValue);
        }
        let item = self.active_item_mut().ok_or(ErrorCode::NoActiveItem)?;
        if item.state != ItemState::Voting {
            return Err(ErrorCode::ItemNotVoting);
        }
        match self.votes.iter_mut().find(|(id, _)| *id == me) {
            Some(vote) => vote.1 = value,
            None => self.votes.push((me, value)),
        }
        Ok(Outcome {
            broadcast: vec![Event::VoteCast { participant_id: me }],
            ..Outcome::default()
        })
    }

    fn reveal(&mut self) -> Result<Outcome, ErrorCode> {
        let has_votes = !self.votes.is_empty();
        let item = self.active_item_mut().ok_or(ErrorCode::NoActiveItem)?;
        if item.state != ItemState::Voting {
            return Err(ErrorCode::ItemNotVoting);
        }
        if !has_votes {
            return Err(ErrorCode::NoVotes);
        }
        item.state = ItemState::Revealed;
        let item_id = item.id;
        Ok(Outcome {
            broadcast: vec![Event::VotesRevealed {
                item_id,
                votes: self.votes.clone(),
            }],
            ..Outcome::default()
        })
    }

    fn activate_item(&mut self, item_id: ItemId) -> Result<Outcome, ErrorCode> {
        let target = self
            .items
            .iter()
            .position(|item| item.id == item_id)
            .ok_or(ErrorCode::UnknownItem)?;
        if self.items[target].state == ItemState::Estimated {
            return Err(ErrorCode::ItemAlreadyEstimated);
        }
        // The spec defines activation only for Pending Items: the Active Item is
        // already activated, so nothing changes and nothing is broadcast.
        if self.active_item_id == Some(item_id) {
            return Ok(Outcome::default());
        }
        // Skipping: the old Active Item goes back to Pending and its Votes are
        // discarded with it.
        self.votes.clear();
        if let Some(old) = self.active_item_mut() {
            old.state = ItemState::Pending;
        }
        self.items[target].state = ItemState::Voting;
        self.active_item_id = Some(item_id);
        Ok(Outcome {
            broadcast: vec![Event::ItemActivated { item_id }],
            ..Outcome::default()
        })
    }

    fn add_item(&mut self, title: String) -> Outcome {
        let id = self.next_item_id;
        self.next_item_id += 1;
        self.items.push(ItemView {
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
            ended: false,
        }
    }

    fn join(&mut self, name: String) -> Outcome {
        let id = self.admit(name.clone());
        Outcome {
            admitted: Some(id),
            reply: vec![Event::RoomSnapshot(self.snapshot_for(id))],
            broadcast: vec![Event::ParticipantJoined {
                participant_id: id,
                name,
            }],
            ended: false,
        }
    }

    fn leave(&mut self, me: ParticipantId) -> Outcome {
        let Some(index) = self.participants.iter().position(|p| p.id == me) else {
            return Outcome::default();
        };
        self.participants.remove(index);
        self.votes.retain(|(id, _)| *id != me);
        let mut broadcast = vec![Event::ParticipantLeft { participant_id: me }];
        // Participants are kept in joining order, so the first is the lowest id.
        if let Some(first) = self.participants.first()
            && me == self.facilitator_id
        {
            self.facilitator_id = first.id;
            broadcast.push(Event::FacilitatorChanged {
                participant_id: first.id,
            });
        }
        Outcome {
            broadcast,
            ended: self.participants.is_empty(),
            ..Outcome::default()
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
            items: self.items.clone(),
            active_item_id: self.active_item_id,
            voted: self.votes.iter().map(|(id, _)| *id).collect(),
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
    fn activating_the_already_active_item_changes_nothing_and_broadcasts_nothing() {
        let mut room = room_with_facilitator();
        add(&mut room, "Login page").unwrap();
        activate(&mut room, 1).unwrap();
        // Stand in for a Revealed Item: re-activation must not reset it to Voting.
        room.items[0].state = ItemState::Revealed;

        let outcome = activate(&mut room, 1).unwrap();

        assert_eq!(outcome, Outcome::default());
        let snapshot = room.snapshot_for(1);
        assert_eq!(snapshot.active_item_id, Some(1));
        assert_eq!(
            snapshot.items,
            vec![item_view(1, "Login page", ItemState::Revealed)]
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

    fn vote(room: &mut Room, me: ParticipantId, value: &str) -> Result<Outcome, ErrorCode> {
        room.apply(
            Actor::Participant(me),
            Command::Vote {
                value: value.to_string(),
            },
        )
    }

    fn reveal(room: &mut Room, me: ParticipantId) -> Result<Outcome, ErrorCode> {
        room.apply(Actor::Participant(me), Command::Reveal)
    }

    fn voting_room() -> Room {
        let mut room = room_with_facilitator();
        add(&mut room, "Login page").unwrap();
        activate(&mut room, 1).unwrap();
        room
    }

    #[test]
    fn voting_broadcasts_vote_cast_naming_the_voter_and_the_facilitator_votes_too() {
        let mut room = voting_room();
        join(&mut room, "Bob");

        let bob = vote(&mut room, 2, "5").unwrap();
        let ada = vote(&mut room, 1, "?").unwrap();

        assert_eq!(bob.broadcast, vec![Event::VoteCast { participant_id: 2 }]);
        assert_eq!(ada.broadcast, vec![Event::VoteCast { participant_id: 1 }]);
        assert_eq!(room.snapshot_for(1).voted, vec![2, 1]);
    }

    #[test]
    fn voting_again_replaces_the_vote_without_duplicating_voted() {
        let mut room = voting_room();
        vote(&mut room, 1, "5").unwrap();

        let again = vote(&mut room, 1, "8").unwrap();

        assert_eq!(again.broadcast, vec![Event::VoteCast { participant_id: 1 }]);
        assert_eq!(room.snapshot_for(1).voted, vec![1]);
        let revealed = reveal(&mut room, 1).unwrap();
        assert_eq!(
            revealed.broadcast,
            vec![Event::VotesRevealed {
                item_id: 1,
                votes: vec![(1, "8".to_string())]
            }]
        );
    }

    #[test]
    fn every_scale_value_is_accepted_and_others_are_invalid() {
        let mut room = voting_room();
        for value in ["0", "1", "2", "3", "5", "8", "13", "20", "40", "100", "?"] {
            assert!(vote(&mut room, 1, value).is_ok(), "{value}");
        }
        for value in ["4", "", "?!", "05", "101", "-1", "a"] {
            assert_eq!(
                vote(&mut room, 1, value).unwrap_err(),
                ErrorCode::InvalidValue,
                "{value}"
            );
        }
    }

    #[test]
    fn voting_with_no_active_item_is_refused() {
        let mut room = room_with_facilitator();
        add(&mut room, "Login page").unwrap();

        assert_eq!(
            vote(&mut room, 1, "5").unwrap_err(),
            ErrorCode::NoActiveItem
        );
    }

    #[test]
    fn voting_on_a_revealed_item_is_refused() {
        let mut room = voting_room();
        vote(&mut room, 1, "5").unwrap();
        reveal(&mut room, 1).unwrap();

        assert_eq!(
            vote(&mut room, 1, "8").unwrap_err(),
            ErrorCode::ItemNotVoting
        );
    }

    #[test]
    fn a_late_joiner_during_voting_can_vote_and_appears_in_voted() {
        let mut room = voting_room();
        vote(&mut room, 1, "5").unwrap();
        let joined = join(&mut room, "Bob");
        let Event::RoomSnapshot(snapshot) = &joined.reply[0] else {
            panic!("expected a snapshot");
        };
        assert_eq!(snapshot.voted, vec![1]);

        vote(&mut room, 2, "3").unwrap();

        assert_eq!(room.snapshot_for(2).voted, vec![1, 2]);
    }

    #[test]
    fn a_late_joiner_after_reveal_sees_a_revealed_item_and_who_voted_but_no_values() {
        let mut room = voting_room();
        vote(&mut room, 1, "5").unwrap();
        reveal(&mut room, 1).unwrap();

        let joined = join(&mut room, "Bob");

        let Event::RoomSnapshot(snapshot) = &joined.reply[0] else {
            panic!("expected a snapshot");
        };
        assert_eq!(
            snapshot.items,
            vec![item_view(1, "Login page", ItemState::Revealed)]
        );
        assert_eq!(snapshot.active_item_id, Some(1));
        assert_eq!(snapshot.voted, vec![1]);
    }

    #[test]
    fn reveal_broadcasts_every_vote_and_makes_the_item_revealed() {
        let mut room = voting_room();
        join(&mut room, "Bob");
        join(&mut room, "Cy");
        vote(&mut room, 2, "13").unwrap();
        vote(&mut room, 1, "5").unwrap();

        let outcome = reveal(&mut room, 1).unwrap();

        assert_eq!(
            outcome.broadcast,
            vec![Event::VotesRevealed {
                item_id: 1,
                votes: vec![(2, "13".to_string()), (1, "5".to_string())]
            }]
        );
        assert_eq!(
            room.snapshot_for(1).items,
            vec![item_view(1, "Login page", ItemState::Revealed)]
        );
    }

    #[test]
    fn reveal_with_no_votes_is_refused_and_a_voter_cannot_reveal() {
        let mut room = voting_room();
        join(&mut room, "Bob");
        assert_eq!(reveal(&mut room, 1).unwrap_err(), ErrorCode::NoVotes);

        vote(&mut room, 1, "5").unwrap();
        assert_eq!(reveal(&mut room, 2).unwrap_err(), ErrorCode::NotFacilitator);
    }

    #[test]
    fn a_participant_leaving_before_reveal_takes_their_vote_with_them() {
        let mut room = voting_room();
        join(&mut room, "Bob");
        vote(&mut room, 1, "5").unwrap();
        vote(&mut room, 2, "8").unwrap();

        leave(&mut room, 2);

        assert_eq!(room.snapshot_for(1).voted, vec![1]);
        assert_eq!(
            reveal(&mut room, 1).unwrap().broadcast,
            vec![Event::VotesRevealed {
                item_id: 1,
                votes: vec![(1, "5".to_string())]
            }]
        );
    }

    #[test]
    fn skipping_the_active_item_discards_its_votes() {
        let mut room = voting_room();
        add(&mut room, "Logout page").unwrap();
        vote(&mut room, 1, "5").unwrap();

        activate(&mut room, 2).unwrap();
        assert!(room.snapshot_for(1).voted.is_empty());

        activate(&mut room, 1).unwrap();
        assert!(room.snapshot_for(1).voted.is_empty());
        assert_eq!(reveal(&mut room, 1).unwrap_err(), ErrorCode::NoVotes);
    }

    #[test]
    fn facilitator_promotion_mid_vote_keeps_votes_and_the_new_facilitator_can_reveal() {
        let mut room = voting_room();
        join(&mut room, "Bob");
        join(&mut room, "Cy");
        vote(&mut room, 2, "8").unwrap();
        vote(&mut room, 3, "3").unwrap();

        leave(&mut room, 1);

        let snapshot = room.snapshot_for(2);
        assert_eq!(snapshot.facilitator_id, 2);
        assert_eq!(snapshot.active_item_id, Some(1));
        assert_eq!(snapshot.voted, vec![2, 3]);
        assert_eq!(
            reveal(&mut room, 2).unwrap().broadcast,
            vec![Event::VotesRevealed {
                item_id: 1,
                votes: vec![(2, "8".to_string()), (3, "3".to_string())]
            }]
        );
    }

    fn revote(room: &mut Room, me: ParticipantId) -> Result<Outcome, ErrorCode> {
        room.apply(Actor::Participant(me), Command::Revote)
    }

    fn accept(room: &mut Room, me: ParticipantId, value: &str) -> Result<Outcome, ErrorCode> {
        room.apply(
            Actor::Participant(me),
            Command::Accept {
                value: value.to_string(),
            },
        )
    }

    fn revealed_room() -> Room {
        let mut room = voting_room();
        join(&mut room, "Bob");
        vote(&mut room, 1, "5").unwrap();
        vote(&mut room, 2, "8").unwrap();
        reveal(&mut room, 1).unwrap();
        room
    }

    #[test]
    fn revote_clears_every_vote_and_returns_the_item_to_voting() {
        let mut room = revealed_room();

        let outcome = revote(&mut room, 1).unwrap();

        assert_eq!(outcome.broadcast, vec![Event::VotesCleared { item_id: 1 }]);
        let snapshot = room.snapshot_for(1);
        assert_eq!(
            snapshot.items,
            vec![item_view(1, "Login page", ItemState::Voting)]
        );
        assert_eq!(snapshot.active_item_id, Some(1));
        assert!(snapshot.voted.is_empty());
        // Voting is open again.
        assert!(vote(&mut room, 2, "3").is_ok());
    }

    #[test]
    fn accept_records_the_result_and_leaves_no_active_item() {
        let mut room = revealed_room();

        let outcome = accept(&mut room, 1, "5").unwrap();

        assert_eq!(
            outcome.broadcast,
            vec![Event::ItemEstimated {
                item_id: 1,
                result: "5".to_string()
            }]
        );
        let snapshot = room.snapshot_for(1);
        assert_eq!(snapshot.active_item_id, None);
        assert!(snapshot.voted.is_empty());
        assert_eq!(
            snapshot.items,
            vec![ItemView {
                result: Some("5".to_string()),
                ..item_view(1, "Login page", ItemState::Estimated)
            }]
        );
    }

    #[test]
    fn accept_takes_a_scale_value_no_participant_voted() {
        let mut room = revealed_room();

        assert!(accept(&mut room, 1, "13").is_ok());
        assert_eq!(room.snapshot_for(1).items[0].result.as_deref(), Some("13"));
    }

    #[test]
    fn a_late_joiner_sees_each_estimated_items_result() {
        let mut room = revealed_room();
        accept(&mut room, 1, "5").unwrap();
        add(&mut room, "Logout page").unwrap();

        let joined = join(&mut room, "Cy");

        let Event::RoomSnapshot(snapshot) = &joined.reply[0] else {
            panic!("expected a snapshot");
        };
        assert_eq!(snapshot.items[0].state, ItemState::Estimated);
        assert_eq!(snapshot.items[0].result.as_deref(), Some("5"));
        assert_eq!(snapshot.items[1].result, None);
    }

    #[test]
    fn revote_and_accept_refuse_an_item_that_is_not_revealed() {
        let mut room = voting_room();
        vote(&mut room, 1, "5").unwrap();

        assert_eq!(
            revote(&mut room, 1).unwrap_err(),
            ErrorCode::ItemNotRevealed
        );
        assert_eq!(
            accept(&mut room, 1, "5").unwrap_err(),
            ErrorCode::ItemNotRevealed
        );
        // Refusals changed nothing.
        assert_eq!(room.snapshot_for(1).voted, vec![1]);
    }

    #[test]
    fn revote_and_accept_with_no_active_item_are_refused() {
        let mut room = room_with_facilitator();
        add(&mut room, "Login page").unwrap();

        assert_eq!(revote(&mut room, 1).unwrap_err(), ErrorCode::NoActiveItem);
        assert_eq!(
            accept(&mut room, 1, "5").unwrap_err(),
            ErrorCode::NoActiveItem
        );
    }

    #[test]
    fn accept_off_the_scale_is_invalid_value_and_the_item_stays_revealed() {
        let mut room = revealed_room();

        for value in ["4", "", "05", "?!"] {
            assert_eq!(
                accept(&mut room, 1, value).unwrap_err(),
                ErrorCode::InvalidValue,
                "{value}"
            );
        }
        assert_eq!(
            room.snapshot_for(1).items,
            vec![item_view(1, "Login page", ItemState::Revealed)]
        );
        assert!(accept(&mut room, 1, "?").is_ok());
    }

    #[test]
    fn after_accept_the_room_refuses_item_commands_and_the_estimated_item_cannot_be_reactivated() {
        let mut room = revealed_room();
        accept(&mut room, 1, "5").unwrap();

        assert_eq!(
            vote(&mut room, 2, "5").unwrap_err(),
            ErrorCode::NoActiveItem
        );
        assert_eq!(reveal(&mut room, 1).unwrap_err(), ErrorCode::NoActiveItem);
        assert_eq!(revote(&mut room, 1).unwrap_err(), ErrorCode::NoActiveItem);
        assert_eq!(
            accept(&mut room, 1, "8").unwrap_err(),
            ErrorCode::NoActiveItem
        );
        assert_eq!(
            activate(&mut room, 1).unwrap_err(),
            ErrorCode::ItemAlreadyEstimated
        );
        // Refusals changed nothing: the Result stands and no Item is active.
        let snapshot = room.snapshot_for(1);
        assert_eq!(snapshot.items[0].result.as_deref(), Some("5"));
        assert_eq!(snapshot.active_item_id, None);
    }

    #[test]
    fn a_voter_cannot_revote_or_accept() {
        let mut room = revealed_room();

        assert_eq!(revote(&mut room, 2).unwrap_err(), ErrorCode::NotFacilitator);
        assert_eq!(
            accept(&mut room, 2, "5").unwrap_err(),
            ErrorCode::NotFacilitator
        );
        assert_eq!(room.snapshot_for(1).voted, vec![1, 2]);
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

    fn join(room: &mut Room, name: &str) -> Outcome {
        room.apply(
            Actor::Newcomer,
            Command::JoinRoom {
                join_code: "ABCD".to_string(),
                name: name.to_string(),
            },
        )
        .unwrap()
    }

    fn leave(room: &mut Room, me: ParticipantId) -> Outcome {
        room.apply(Actor::Participant(me), Command::Leave).unwrap()
    }

    fn participant(id: ParticipantId, name: &str) -> Participant {
        Participant {
            id,
            name: name.to_string(),
        }
    }

    #[test]
    fn join_room_admits_a_voter_with_a_snapshot_and_broadcasts_participant_joined() {
        let mut room = room_with_facilitator();
        add(&mut room, "Login page").unwrap();
        activate(&mut room, 1).unwrap();

        let outcome = join(&mut room, "Bob");

        assert_eq!(
            outcome,
            Outcome {
                admitted: Some(2),
                reply: vec![Event::RoomSnapshot(Snapshot {
                    join_code: "ABCD".to_string(),
                    you: 2,
                    facilitator_id: 1,
                    participants: vec![participant(1, "Ada"), participant(2, "Bob")],
                    items: vec![item_view(1, "Login page", ItemState::Voting)],
                    active_item_id: Some(1),
                    voted: vec![],
                })],
                broadcast: vec![Event::ParticipantJoined {
                    participant_id: 2,
                    name: "Bob".to_string()
                }],
                ended: false,
            }
        );
    }

    #[test]
    fn two_participants_may_share_a_display_name() {
        let mut room = room_with_facilitator();

        let outcome = join(&mut room, "Ada");

        assert_eq!(outcome.admitted, Some(2));
        assert_eq!(
            room.snapshot_for(2).participants,
            vec![participant(1, "Ada"), participant(2, "Ada")]
        );
    }

    #[test]
    fn a_voter_leaving_broadcasts_participant_left_and_the_facilitator_stays() {
        let mut room = room_with_facilitator();
        join(&mut room, "Bob");
        join(&mut room, "Cy");

        let outcome = leave(&mut room, 2);

        assert_eq!(
            outcome,
            Outcome {
                broadcast: vec![Event::ParticipantLeft { participant_id: 2 }],
                ..Outcome::default()
            }
        );
        let snapshot = room.snapshot_for(1);
        assert_eq!(snapshot.facilitator_id, 1);
        assert_eq!(
            snapshot.participants,
            vec![participant(1, "Ada"), participant(3, "Cy")]
        );
    }

    #[test]
    fn the_facilitator_leaving_promotes_the_lowest_remaining_id() {
        let mut room = room_with_facilitator();
        join(&mut room, "Bob");
        join(&mut room, "Cy");
        join(&mut room, "Dee");
        leave(&mut room, 2);

        let outcome = leave(&mut room, 1);

        assert_eq!(
            outcome.broadcast,
            vec![
                Event::ParticipantLeft { participant_id: 1 },
                Event::FacilitatorChanged { participant_id: 3 },
            ]
        );
        assert!(!outcome.ended);
        assert_eq!(room.snapshot_for(3).facilitator_id, 3);
    }

    #[test]
    fn a_promoted_facilitator_may_steer_and_the_active_item_survives() {
        let mut room = room_with_facilitator();
        add(&mut room, "Login page").unwrap();
        activate(&mut room, 1).unwrap();
        join(&mut room, "Bob");

        leave(&mut room, 1);

        assert_eq!(room.snapshot_for(2).active_item_id, Some(1));
        assert!(
            room.apply(
                Actor::Participant(2),
                Command::AddItem {
                    title: "Next".to_string()
                }
            )
            .is_ok()
        );
    }

    #[test]
    fn the_last_participant_leaving_reports_the_room_ended() {
        let mut room = room_with_facilitator();
        join(&mut room, "Bob");
        assert!(!leave(&mut room, 1).ended);

        let outcome = leave(&mut room, 2);

        assert_eq!(
            outcome,
            Outcome {
                broadcast: vec![Event::ParticipantLeft { participant_id: 2 }],
                ended: true,
                ..Outcome::default()
            }
        );
    }

    #[test]
    fn leaving_as_someone_not_in_the_room_changes_nothing_and_does_not_report_ended() {
        let mut room = room_with_facilitator();
        leave(&mut room, 1);

        assert_eq!(leave(&mut room, 1), Outcome::default());
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
                    voted: vec![],
                })],
                ended: false,
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
