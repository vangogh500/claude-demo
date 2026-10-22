//! Protocol seam: the real binary on an ephemeral port, driven by websocket clients.

use std::process::Stdio;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type Client = WebSocketStream<MaybeTlsStream<TcpStream>>;

const JOIN_CODE_ALPHABET: &str = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

/// A running server process, killed when dropped.
struct Server {
    _child: Child,
    addr: String,
}

/// Starts the binary with the given flag arguments and `BIND` value, and waits for
/// the address it reports listening on.
async fn start_server(args: &[&str], env_bind: Option<&str>) -> Server {
    let mut command = Command::new(env!("CARGO_BIN_EXE_story-pointing"));
    command
        .args(args)
        .env_remove("BIND")
        .stdout(Stdio::piped())
        .kill_on_drop(true);
    if let Some(bind) = env_bind {
        command.env("BIND", bind);
    }
    let mut child = command.spawn().expect("server binary starts");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout is piped"));
    let mut line = String::new();
    tokio::time::timeout(Duration::from_secs(10), stdout.read_line(&mut line))
        .await
        .expect("server reports its address in time")
        .expect("stdout is readable");
    // Keep the pipe open and drained, so later server output neither fails nor blocks.
    tokio::spawn(async move {
        let mut lines = stdout.lines();
        while let Ok(Some(_)) = lines.next_line().await {}
    });
    let addr = line
        .trim()
        .strip_prefix("listening on ")
        .unwrap_or_else(|| panic!("unexpected first line: {line:?}"))
        .to_string();
    Server {
        _child: child,
        addr,
    }
}

async fn ephemeral_server() -> Server {
    start_server(&["--bind", "127.0.0.1:0"], None).await
}

async fn connect(server: &Server) -> Client {
    let (client, _) = tokio_tungstenite::connect_async(format!("ws://{}/ws", server.addr))
        .await
        .expect("websocket handshake succeeds");
    client
}

async fn send(client: &mut Client, frame: Value) {
    client.send(Message::text(frame.to_string())).await.unwrap();
}

async fn receive(client: &mut Client) -> Value {
    loop {
        let message = tokio::time::timeout(Duration::from_secs(5), client.next())
            .await
            .expect("a frame arrives in time")
            .expect("the socket is still open")
            .expect("the frame is readable");
        match message {
            Message::Text(text) => return serde_json::from_str(&text).expect("frame is JSON"),
            Message::Ping(_) | Message::Pong(_) => continue,
            other => panic!("expected a text frame, got {other:?}"),
        }
    }
}

fn assert_join_code_shape(code: &str) {
    assert_eq!(
        code.chars().count(),
        4,
        "join code {code:?} has four characters"
    );
    assert!(
        code.chars().all(|c| JOIN_CODE_ALPHABET.contains(c)),
        "join code {code:?} uses only the unambiguous alphabet"
    );
}

#[tokio::test]
async fn create_room_answers_the_creator_with_a_snapshot() {
    let server = ephemeral_server().await;
    let mut ada = connect(&server).await;

    send(&mut ada, json!({"type": "create_room", "name": "Ada"})).await;
    let snapshot = receive(&mut ada).await;

    assert_eq!(snapshot["type"], "room_snapshot");
    assert_join_code_shape(snapshot["join_code"].as_str().unwrap());
    assert_eq!(snapshot["you"], snapshot["facilitator_id"]);
    assert_eq!(
        snapshot["participants"],
        json!([{"id": snapshot["you"], "name": "Ada"}])
    );
    assert_eq!(snapshot["items"], json!([]));
    assert_eq!(snapshot["active_item_id"], Value::Null);
    assert_eq!(snapshot["voted"], json!([]));
}

#[tokio::test]
async fn the_server_listens_on_bind_from_the_environment() {
    let server = start_server(&[], Some("127.0.0.1:0")).await;
    let mut ada = connect(&server).await;

    send(&mut ada, json!({"type": "create_room", "name": "Ada"})).await;

    assert_eq!(receive(&mut ada).await["type"], "room_snapshot");
}

#[tokio::test]
async fn rooms_created_concurrently_get_different_join_codes() {
    let server = ephemeral_server().await;
    let mut ada = connect(&server).await;
    let mut bob = connect(&server).await;

    tokio::join!(
        send(&mut ada, json!({"type": "create_room", "name": "Ada"})),
        send(&mut bob, json!({"type": "create_room", "name": "Bob"})),
    );
    let (ada_snapshot, bob_snapshot) = tokio::join!(receive(&mut ada), receive(&mut bob));

    assert_join_code_shape(ada_snapshot["join_code"].as_str().unwrap());
    assert_join_code_shape(bob_snapshot["join_code"].as_str().unwrap());
    assert_ne!(ada_snapshot["join_code"], bob_snapshot["join_code"]);
}

#[tokio::test]
async fn any_other_command_before_create_or_join_is_refused_as_not_in_room() {
    let server = ephemeral_server().await;
    let mut ada = connect(&server).await;
    let not_in_room = json!({"type": "error", "code": "not_in_room"});

    for command in [
        json!({"type": "add_item", "title": "Login page"}),
        json!({"type": "activate_item", "item_id": 1}),
        json!({"type": "vote", "value": "5"}),
        json!({"type": "reveal"}),
        json!({"type": "revote"}),
        json!({"type": "accept", "value": "8"}),
    ] {
        send(&mut ada, command.clone()).await;
        assert_eq!(
            receive(&mut ada).await,
            not_in_room,
            "first message {command}"
        );
    }

    send(&mut ada, json!({"type": "create_room", "name": "Ada"})).await;
    assert_eq!(receive(&mut ada).await["type"], "room_snapshot");
}

#[tokio::test]
async fn a_second_create_room_on_a_bound_socket_is_refused() {
    let server = ephemeral_server().await;
    let mut ada = connect(&server).await;
    send(&mut ada, json!({"type": "create_room", "name": "Ada"})).await;
    receive(&mut ada).await;

    send(&mut ada, json!({"type": "create_room", "name": "Ada"})).await;

    assert_eq!(
        receive(&mut ada).await,
        json!({"type": "error", "code": "already_in_room"})
    );
}

#[tokio::test]
async fn malformed_frames_are_refused_and_the_socket_stays_open() {
    let server = ephemeral_server().await;
    let mut ada = connect(&server).await;
    let malformed = json!({"type": "error", "code": "malformed"});

    ada.send(Message::text("not json")).await.unwrap();
    assert_eq!(receive(&mut ada).await, malformed);
    send(&mut ada, json!({"type": "dance"})).await;
    assert_eq!(receive(&mut ada).await, malformed);
    send(&mut ada, json!({"type": "create_room"})).await;
    assert_eq!(receive(&mut ada).await, malformed);
    ada.send(Message::binary(vec![1, 2, 3])).await.unwrap();
    assert_eq!(receive(&mut ada).await, malformed);

    send(&mut ada, json!({"type": "create_room", "name": "Ada"})).await;
    assert_eq!(receive(&mut ada).await["type"], "room_snapshot");
}
