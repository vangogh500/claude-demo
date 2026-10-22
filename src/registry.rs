//! The Room registry: every live Room in the process, keyed by Join Code.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rand::Rng;

use crate::room::{Actor, Command, ErrorCode, Outcome, Room};

/// The characters a Join Code is drawn from: uppercase letters and digits without
/// the look-alikes `0`/`O` and `1`/`I`.
pub const JOIN_CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const JOIN_CODE_LEN: usize = 4;

/// A Room shared by the sockets bound to it. Each [`HostedRoom::apply`] holds the
/// Room's lock for the length of one command, so commands apply one at a time.
pub struct HostedRoom {
    room: Mutex<Room>,
}

impl HostedRoom {
    pub fn apply(&self, actor: Actor, command: Command) -> Result<Outcome, ErrorCode> {
        self.room.lock().expect("room lock").apply(actor, command)
    }
}

#[derive(Default)]
pub struct Registry {
    rooms: Mutex<HashMap<String, Arc<HostedRoom>>>,
}

impl Registry {
    /// Creates a Room under a Join Code no live Room holds, with `name` as its
    /// Facilitator, and returns it with the outcome of `create_room`.
    pub fn create_room(&self, name: String) -> (Arc<HostedRoom>, Outcome) {
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
            .expect("a new Room admits its creator");
        let hosted = Arc::new(HostedRoom {
            room: Mutex::new(room),
        });
        rooms.insert(join_code, Arc::clone(&hosted));
        (hosted, outcome)
    }
}

fn random_join_code() -> String {
    let mut rng = rand::rng();
    (0..JOIN_CODE_LEN)
        .map(|_| JOIN_CODE_ALPHABET[rng.random_range(0..JOIN_CODE_ALPHABET.len())] as char)
        .collect()
}
