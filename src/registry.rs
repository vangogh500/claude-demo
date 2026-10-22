//! The Room registry: every live Room in the process, keyed by Join Code, and the
//! sockets attached to each.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rand::Rng;
use tokio::sync::mpsc::UnboundedSender;

use crate::protocol;
use crate::room::{Actor, Command, ErrorCode, Outcome, ParticipantId, Room};

/// The characters a Join Code is drawn from: uppercase letters and digits without
/// the look-alikes `0`/`O` and `1`/`I`.
pub const JOIN_CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const JOIN_CODE_LEN: usize = 4;

/// Where a socket bound to a Room receives its frames: the reply to its own
/// command and every broadcast in the Room, in the order the Room produced them.
/// The Room drops the sender when the socket's Participant leaves or the Room ends.
pub type Outbox = UnboundedSender<String>;

/// A Room shared by the sockets bound to it. Each command holds the Room's lock for
/// its whole length, applying it and queueing its frames, so commands apply one at
/// a time and every socket sees the Room's events in the same order.
pub struct HostedRoom {
    join_code: String,
    state: Mutex<Hosted>,
}

struct Hosted {
    room: Room,
    outboxes: HashMap<ParticipantId, Outbox>,
    ended: bool,
}

impl HostedRoom {
    /// Applies `command` from Participant `me`, queueing the frames it produces. A
    /// refusal is returned, not queued: it is for the sender only.
    pub fn apply(&self, me: ParticipantId, command: Command) -> Result<(), ErrorCode> {
        let mut hosted = self.state.lock().expect("room lock");
        let outcome = hosted.room.apply(Actor::Participant(me), command)?;
        hosted.deliver(me, &outcome);
        Ok(())
    }
}

impl Hosted {
    /// Queues `outcome`'s reply for `sender` and its broadcast for every socket.
    fn deliver(&self, sender: ParticipantId, outcome: &Outcome) {
        if let Some(outbox) = self.outboxes.get(&sender) {
            for event in &outcome.reply {
                let _ = outbox.send(protocol::encode_event(event));
            }
        }
        for event in &outcome.broadcast {
            let frame = protocol::encode_event(event);
            for outbox in self.outboxes.values() {
                let _ = outbox.send(frame.clone());
            }
        }
    }
}

#[derive(Default)]
pub struct Registry {
    rooms: Mutex<HashMap<String, Arc<HostedRoom>>>,
}

impl Registry {
    /// Creates a Room under a Join Code no live Room holds, with `name` as its
    /// Facilitator delivered to `outbox`, and returns it with the creator's id.
    pub fn create_room(&self, name: String, outbox: Outbox) -> (Arc<HostedRoom>, ParticipantId) {
        let mut rooms = self.rooms.lock().expect("registry lock");
        let join_code = loop {
            let candidate = random_join_code();
            if !rooms.contains_key(&candidate) {
                break candidate;
            }
        };
        let mut room = Room::new(join_code.clone());
        let outcome = room
            .apply(Actor::Newcomer, Command::CreateRoom { name })
            .expect("a new Room accepts create_room");
        let creator = outcome.admitted.expect("a new Room admits its creator");
        let hosted = Hosted {
            room,
            outboxes: HashMap::from([(creator, outbox)]),
            ended: false,
        };
        hosted.deliver(creator, &outcome);
        let hosted = Arc::new(HostedRoom {
            join_code: join_code.clone(),
            state: Mutex::new(hosted),
        });
        rooms.insert(join_code, Arc::clone(&hosted));
        (hosted, creator)
    }

    /// Admits `name` to the live Room under `join_code`, delivering to `outbox`, and
    /// returns the Room with the joiner's id; `unknown_join_code` if no live Room
    /// holds that code.
    pub fn join_room(
        &self,
        join_code: String,
        name: String,
        outbox: Outbox,
    ) -> Result<(Arc<HostedRoom>, ParticipantId), ErrorCode> {
        let rooms = self.rooms.lock().expect("registry lock");
        let room = rooms
            .get(&join_code)
            .cloned()
            .ok_or(ErrorCode::UnknownJoinCode)?;
        let mut hosted = room.state.lock().expect("room lock");
        // A Room that has ended but is not yet dropped from the map is not live.
        if hosted.ended {
            return Err(ErrorCode::UnknownJoinCode);
        }
        let outcome = hosted
            .room
            .apply(Actor::Newcomer, Command::JoinRoom { join_code, name })?;
        let me = outcome.admitted.expect("join_room admits the joiner");
        hosted.outboxes.insert(me, outbox);
        hosted.deliver(me, &outcome);
        drop(hosted);
        Ok((room, me))
    }

    /// Takes Participant `me` out of `room`, announcing it to the rest of the Room.
    /// If that ends the Room, drops every remaining outbox (closing any socket still
    /// attached) and releases the Join Code.
    pub fn leave(&self, room: &Arc<HostedRoom>, me: ParticipantId) {
        let mut hosted = room.state.lock().expect("room lock");
        if hosted.ended {
            return;
        }
        hosted.outboxes.remove(&me);
        let outcome = hosted
            .room
            .apply(Actor::Participant(me), Command::Leave)
            .expect("leave is never refused");
        hosted.deliver(me, &outcome);
        if !outcome.ended {
            return;
        }
        hosted.ended = true;
        hosted.outboxes.clear();
        drop(hosted);
        let mut rooms = self.rooms.lock().expect("registry lock");
        // Only release the code if it still names this Room: once it is released, a
        // newer Room may be issued the same code, and that one must stay.
        if rooms
            .get(&room.join_code)
            .is_some_and(|current| Arc::ptr_eq(current, room))
        {
            rooms.remove(&room.join_code);
        }
    }
}

fn random_join_code() -> String {
    let mut rng = rand::rng();
    (0..JOIN_CODE_LEN)
        .map(|_| JOIN_CODE_ALPHABET[rng.random_range(0..JOIN_CODE_ALPHABET.len())] as char)
        .collect()
}
