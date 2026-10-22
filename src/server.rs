//! The websocket layer: serves `GET /ws`, decodes each text frame into a command,
//! applies it to the socket's Room and writes the reply back as frames.
//!
//! A socket starts unbound. While unbound, `create_room` creates a Room and binds
//! the socket to it as the Participant the Room admitted; `join_room` is not
//! modelled yet and gets no reply; any other command gets `not_in_room`. Once
//! bound, every command is applied to that Room as that Participant, so a second
//! `create_room` or `join_room` gets the Room's `already_in_room`. A frame that does
//! not decode gets `malformed`. Errors go to the sending socket only, and no error
//! closes the socket.

use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use axum::routing::get;
use tokio::net::TcpListener;

use crate::protocol;
use crate::registry::{HostedRoom, Registry};
use crate::room::{Actor, Command, ErrorCode, Outcome, ParticipantId};

/// Serves `GET /ws` on `listener` until the process ends, with a fresh, empty
/// registry of Rooms.
pub async fn serve(listener: TcpListener) -> std::io::Result<()> {
    let app = Router::new()
        .route("/ws", get(upgrade))
        .with_state(Arc::new(Registry::default()));
    axum::serve(listener, app).await
}

async fn upgrade(ws: WebSocketUpgrade, State(registry): State<Arc<Registry>>) -> Response {
    ws.on_upgrade(move |socket| run_socket(socket, registry))
}

/// The Room and Participant a socket is bound to after its first accepted command.
type Binding = Option<(Arc<HostedRoom>, ParticipantId)>;

async fn run_socket(mut socket: WebSocket, registry: Arc<Registry>) {
    let mut binding: Binding = None;
    while let Some(Ok(message)) = socket.recv().await {
        let reply = match message {
            Message::Text(text) => match protocol::decode(text.as_str()) {
                Ok(command) => handle(&registry, &mut binding, command),
                Err(protocol::Malformed) => vec![protocol::encode_malformed()],
            },
            Message::Binary(_) => vec![protocol::encode_malformed()],
            Message::Ping(_) | Message::Pong(_) => continue,
            Message::Close(_) => break,
        };
        for frame in reply {
            if socket.send(Message::text(frame)).await.is_err() {
                return;
            }
        }
    }
    // Leaving a Room is not modelled yet: the Room keeps this socket's Participant
    // and its Join Code after the socket goes.
}

/// Applies `command` for a socket in state `binding`, binding it when the command
/// admits it to a Room, and returns the frames to send back to that socket.
fn handle(registry: &Registry, binding: &mut Binding, command: Command) -> Vec<String> {
    let result = match (&*binding, command) {
        (None, Command::CreateRoom { name }) => {
            let (room, outcome) = registry.create_room(name);
            bind(binding, room, &outcome);
            Ok(outcome)
        }
        // Joining is not modelled yet: the socket gets no reply and stays unbound.
        (None, Command::JoinRoom { .. }) => Ok(Outcome::default()),
        (None, _) => Err(ErrorCode::NotInRoom),
        (Some((room, me)), command) => room.apply(Actor::Participant(*me), command),
    };
    match result {
        Ok(outcome) => outcome.reply.iter().map(protocol::encode_event).collect(),
        Err(code) => vec![protocol::encode_error(code)],
    }
}

fn bind(binding: &mut Binding, room: Arc<HostedRoom>, outcome: &Outcome) {
    if let Some(me) = outcome.admitted {
        *binding = Some((room, me));
    }
}
