//! The server layer: serves the Web Client page at `GET /`, and the websocket layer
//! at `GET /ws`, which decodes each text frame into a command,
//! applies it to the socket's Room and writes the Room's events out as frames.
//!
//! A socket starts unbound. While unbound, `create_room` creates a Room and
//! `join_room` joins the live Room under its Join Code (or gets
//! `unknown_join_code`); either binds the socket to that Room as the Participant
//! the Room admitted. Any other command while unbound gets `not_in_room`. Once
//! bound, every command is applied to that Room as that Participant, so a second
//! `create_room` or `join_room` gets the Room's `already_in_room`. A frame that does
//! not decode gets `malformed`. Errors go to the sending socket only, and no error
//! closes the socket. Every event a Room broadcasts reaches every socket bound to
//! it, the sender included.
//!
//! A bound socket closing is its Participant leaving the Room. When that leaves the
//! Room empty, the Room ends and its Join Code is released; the server would close
//! any other socket still bound to it, though a Room only empties once every bound
//! socket has gone.

use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::{Html, Response};
use axum::routing::get;
use tokio::net::TcpListener;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use crate::protocol;
use crate::registry::{HostedRoom, Registry};
use crate::room::{Command, ErrorCode, ParticipantId};

/// The Web Client: one hand-written page, compiled in (ADR 0002).
const WEB_CLIENT: &str = include_str!("web_client.html");

/// Serves `GET /` (the Web Client) and `GET /ws` on `listener` until the process
/// ends, with a fresh, empty registry of Rooms.
pub async fn serve(listener: TcpListener) -> std::io::Result<()> {
    let app = Router::new()
        .route("/", get(web_client))
        .route("/ws", get(upgrade))
        .with_state(Arc::new(Registry::default()));
    axum::serve(listener, app).await
}

async fn web_client() -> Html<&'static str> {
    Html(WEB_CLIENT)
}

async fn upgrade(ws: WebSocketUpgrade, State(registry): State<Arc<Registry>>) -> Response {
    ws.on_upgrade(move |socket| run_socket(socket, registry))
}

/// The Room and Participant a socket is bound to, and where the Room queues the
/// frames for it.
struct Bound {
    room: Arc<HostedRoom>,
    me: ParticipantId,
    inbox: UnboundedReceiver<String>,
}

async fn run_socket(mut socket: WebSocket, registry: Arc<Registry>) {
    let mut bound: Option<Bound> = None;
    loop {
        tokio::select! {
            // Queued frames first: an error frame goes straight to the socket, so it
            // must not overtake the frames of commands this socket sent before it.
            biased;
            frame = next_frame(&mut bound) => match frame {
                Some(frame) => {
                    if socket.send(Message::text(frame)).await.is_err() {
                        break;
                    }
                }
                // Defensive: the Registry dropped this socket's outbox, which it does
                // only when the Room ends, and a Room ends only once every bound
                // socket has left.
                None => {
                    let _ = socket.send(Message::Close(None)).await;
                    break;
                }
            },
            message = socket.recv() => {
                let reply = match message {
                    Some(Ok(Message::Text(text))) => match protocol::decode(text.as_str()) {
                        Ok(command) => handle(&registry, &mut bound, command),
                        Err(protocol::Malformed) => Some(protocol::encode_malformed()),
                    },
                    Some(Ok(Message::Binary(_))) => Some(protocol::encode_malformed()),
                    Some(Ok(Message::Ping(_) | Message::Pong(_))) => None,
                    Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                };
                if let Some(frame) = reply
                    && socket.send(Message::text(frame)).await.is_err()
                {
                    break;
                }
            }
        }
    }
    if let Some(Bound { room, me, .. }) = bound {
        registry.leave(&room, me);
    }
}

/// The next frame the Room queued for a bound socket; never ready for an unbound one.
async fn next_frame(bound: &mut Option<Bound>) -> Option<String> {
    match bound {
        Some(bound) => bound.inbox.recv().await,
        None => std::future::pending().await,
    }
}

/// Applies `command` for a socket, binding it when the command admits it to a Room,
/// and returns the error frame to send that socket alone, if the command was
/// refused. Accepted commands' frames arrive through the Room's queue.
fn handle(registry: &Registry, bound: &mut Option<Bound>, command: Command) -> Option<String> {
    let result = match (&*bound, command) {
        (None, Command::CreateRoom { name }) => {
            let (outbox, inbox) = unbounded_channel();
            let (room, me) = registry.create_room(name, outbox);
            *bound = Some(Bound { room, me, inbox });
            Ok(())
        }
        (None, Command::JoinRoom { join_code, name }) => {
            let (outbox, inbox) = unbounded_channel();
            registry
                .join_room(join_code, name, outbox)
                .map(|(room, me)| *bound = Some(Bound { room, me, inbox }))
        }
        (None, _) => Err(ErrorCode::NotInRoom),
        (Some(Bound { room, me, .. }), command) => room.apply(*me, command),
    };
    result.err().map(protocol::encode_error)
}
