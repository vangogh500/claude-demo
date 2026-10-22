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

#[tokio::test]
async fn the_facilitator_adds_and_activates_items_and_hears_the_events() {
    let server = ephemeral_server().await;
    let mut ada = connect(&server).await;
    send(&mut ada, json!({"type": "create_room", "name": "Ada"})).await;
    receive(&mut ada).await;

    send(&mut ada, json!({"type": "add_item", "title": "Login page"})).await;
    assert_eq!(
        receive(&mut ada).await,
        json!({"type": "item_added", "item_id": 1, "title": "Login page"})
    );
    send(&mut ada, json!({"type": "activate_item", "item_id": 1})).await;
    assert_eq!(
        receive(&mut ada).await,
        json!({"type": "item_activated", "item_id": 1})
    );
    send(&mut ada, json!({"type": "activate_item", "item_id": 7})).await;
    assert_eq!(
        receive(&mut ada).await,
        json!({"type": "error", "code": "unknown_item"})
    );
}

/// Asserts that `client`'s next frame is `expected`. Used after an action whose
/// event must reach `client` to show that nothing else reached it first.
async fn assert_next(client: &mut Client, expected: Value) {
    assert_eq!(receive(client).await, expected);
}

/// Creates a Room as `name` and returns the client, its participant id and the
/// Room's Join Code.
async fn create(server: &Server, name: &str) -> (Client, Value, String) {
    let mut client = connect(server).await;
    send(&mut client, json!({"type": "create_room", "name": name})).await;
    let snapshot = receive(&mut client).await;
    let code = snapshot["join_code"].as_str().unwrap().to_string();
    (client, snapshot["you"].clone(), code)
}

/// Joins the Room under `code` as `name` and returns the client and its snapshot,
/// leaving its own `participant_joined` unread.
async fn join(server: &Server, code: &str, name: &str) -> (Client, Value) {
    let mut client = connect(server).await;
    send(
        &mut client,
        json!({"type": "join_room", "join_code": code, "name": name}),
    )
    .await;
    let snapshot = receive(&mut client).await;
    assert_eq!(snapshot["type"], "room_snapshot");
    (client, snapshot)
}

/// Closes `client` and waits for the server to finish the close handshake.
async fn close(mut client: Client) {
    client.close(None).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(Ok(_)) = client.next().await {}
    })
    .await
    .expect("the server ends the connection in time");
}

#[tokio::test]
async fn a_joiner_gets_a_snapshot_and_everyone_hears_participant_joined() {
    let server = ephemeral_server().await;
    let (mut ada, ada_id, code) = create(&server, "Ada").await;
    send(&mut ada, json!({"type": "add_item", "title": "Login page"})).await;
    receive(&mut ada).await;

    let (mut bob, snapshot) = join(&server, &code, "Bob").await;

    let bob_id = snapshot["you"].clone();
    assert_ne!(bob_id, ada_id);
    assert_eq!(snapshot["join_code"], code.as_str());
    assert_eq!(snapshot["facilitator_id"], ada_id);
    assert_eq!(
        snapshot["participants"],
        json!([{"id": ada_id, "name": "Ada"}, {"id": bob_id, "name": "Bob"}])
    );
    assert_eq!(
        snapshot["items"],
        json!([{"id": 1, "title": "Login page", "state": "Pending", "result": null}])
    );
    let joined = json!({"type": "participant_joined", "participant_id": bob_id, "name": "Bob"});
    assert_next(&mut bob, joined.clone()).await;
    assert_next(&mut ada, joined).await;
}

#[tokio::test]
async fn two_participants_may_share_a_display_name() {
    let server = ephemeral_server().await;
    let (mut ada, ada_id, code) = create(&server, "Ada").await;

    let (_other_ada, snapshot) = join(&server, &code, "Ada").await;

    let other_id = snapshot["you"].clone();
    assert_ne!(other_id, ada_id);
    assert_next(
        &mut ada,
        json!({"type": "participant_joined", "participant_id": other_id, "name": "Ada"}),
    )
    .await;
}

#[tokio::test]
async fn an_unknown_join_code_is_refused_to_that_socket_only() {
    let server = ephemeral_server().await;
    let (mut ada, _, code) = create(&server, "Ada").await;
    let mut bob = connect(&server).await;

    // `0` and `O` are never issued, so this code cannot name a live Room.
    send(
        &mut bob,
        json!({"type": "join_room", "join_code": "0O0O", "name": "Bob"}),
    )
    .await;

    assert_next(
        &mut bob,
        json!({"type": "error", "code": "unknown_join_code"}),
    )
    .await;
    // Ada's next frame is her own Item: nothing about Bob reached her.
    send(&mut ada, json!({"type": "add_item", "title": "Login page"})).await;
    assert_next(
        &mut ada,
        json!({"type": "item_added", "item_id": 1, "title": "Login page"}),
    )
    .await;
    // Bob is still unbound and can join for real.
    send(
        &mut bob,
        json!({"type": "join_room", "join_code": code, "name": "Bob"}),
    )
    .await;
    assert_eq!(receive(&mut bob).await["type"], "room_snapshot");
}

#[tokio::test]
async fn an_error_inside_a_room_reaches_only_the_sender() {
    let server = ephemeral_server().await;
    let (mut ada, _, code) = create(&server, "Ada").await;
    let (mut bob, _) = join(&server, &code, "Bob").await;
    receive(&mut bob).await;
    receive(&mut ada).await;

    send(&mut bob, json!({"type": "add_item", "title": "Nope"})).await;

    assert_next(
        &mut bob,
        json!({"type": "error", "code": "not_facilitator"}),
    )
    .await;
    // Each socket's next frame is Ada's Item: Bob's error reached neither again.
    send(&mut ada, json!({"type": "add_item", "title": "Login page"})).await;
    let added = json!({"type": "item_added", "item_id": 1, "title": "Login page"});
    assert_next(&mut ada, added.clone()).await;
    assert_next(&mut bob, added).await;
}

#[tokio::test]
async fn a_pipelined_error_arrives_after_the_frames_of_earlier_commands() {
    let server = ephemeral_server().await;
    let (_ada, _, code) = create(&server, "Ada").await;

    // Several joiners, so an ordering the server leaves to chance shows up. Each
    // stays connected until the end: a joiner leaving mid-loop would put a
    // participant_left, at an uncontrolled moment, into the next one's frames.
    let mut joiners = Vec::new();
    for _ in 0..10 {
        let mut bob = connect(&server).await;
        // Both frames in one write, with no receive between them: the refusal of
        // the second must not overtake the frames the first produced.
        let join = json!({"type": "join_room", "join_code": code, "name": "Bob"});
        let add = json!({"type": "add_item", "title": "Nope"});
        bob.feed(Message::text(join.to_string())).await.unwrap();
        bob.feed(Message::text(add.to_string())).await.unwrap();
        bob.flush().await.unwrap();

        assert_eq!(receive(&mut bob).await["type"], "room_snapshot");
        assert_eq!(receive(&mut bob).await["type"], "participant_joined");
        assert_next(
            &mut bob,
            json!({"type": "error", "code": "not_facilitator"}),
        )
        .await;
        joiners.push(bob);
    }
}

#[tokio::test]
async fn a_voter_disconnecting_is_announced_to_the_rest_of_the_room() {
    let server = ephemeral_server().await;
    let (mut ada, _, code) = create(&server, "Ada").await;
    let (bob, snapshot) = join(&server, &code, "Bob").await;
    receive(&mut ada).await;

    close(bob).await;

    assert_next(
        &mut ada,
        json!({"type": "participant_left", "participant_id": snapshot["you"]}),
    )
    .await;
}

#[tokio::test]
async fn the_facilitator_disconnecting_promotes_the_lowest_remaining_id() {
    let server = ephemeral_server().await;
    let (ada, ada_id, code) = create(&server, "Ada").await;
    let (mut bob, bob_snapshot) = join(&server, &code, "Bob").await;
    let (mut cy, _) = join(&server, &code, "Cy").await;
    receive(&mut bob).await; // Bob joined
    receive(&mut bob).await; // Cy joined
    receive(&mut cy).await; // Cy joined

    close(ada).await;

    let left = json!({"type": "participant_left", "participant_id": ada_id});
    let promoted = json!({"type": "facilitator_changed", "participant_id": bob_snapshot["you"]});
    for client in [&mut bob, &mut cy] {
        assert_next(client, left.clone()).await;
        assert_next(client, promoted.clone()).await;
    }
    // The promoted Bob now steers the Room.
    send(&mut bob, json!({"type": "add_item", "title": "Login page"})).await;
    assert_next(
        &mut bob,
        json!({"type": "item_added", "item_id": 1, "title": "Login page"}),
    )
    .await;
}

// A Room ends only when its last socket leaves, so no other socket is ever still
// attached when it does: what a client can observe is that the leaver's connection
// is ended and the Join Code stops resolving.
#[tokio::test]
async fn the_last_participant_leaving_ends_the_room_and_releases_its_join_code() {
    let server = ephemeral_server().await;
    let (mut ada, _, code) = create(&server, "Ada").await;
    let (bob, _) = join(&server, &code, "Bob").await;
    receive(&mut ada).await;

    close(bob).await;
    receive(&mut ada).await; // Bob left
    close(ada).await;

    let mut late = connect(&server).await;
    send(
        &mut late,
        json!({"type": "join_room", "join_code": code, "name": "Late"}),
    )
    .await;
    assert_next(
        &mut late,
        json!({"type": "error", "code": "unknown_join_code"}),
    )
    .await;
}

#[tokio::test]
async fn votes_are_hidden_until_the_facilitator_reveals_them() {
    let server = ephemeral_server().await;
    let (mut ada, _, code) = create(&server, "Ada").await;
    send(&mut ada, json!({"type": "add_item", "title": "Login page"})).await;
    send(&mut ada, json!({"type": "activate_item", "item_id": 1})).await;
    receive(&mut ada).await;
    receive(&mut ada).await;
    send(&mut ada, json!({"type": "vote", "value": "5"})).await;
    assert_next(&mut ada, json!({"type": "vote_cast", "participant_id": 1})).await;

    // A late joiner sees who has voted, and can vote too.
    let (mut bob, snapshot) = join(&server, &code, "Bob").await;
    assert_eq!(snapshot["voted"], json!([1]));
    assert_next(
        &mut bob,
        json!({"type": "participant_joined", "participant_id": 2, "name": "Bob"}),
    )
    .await;
    receive(&mut ada).await;
    send(&mut bob, json!({"type": "vote", "value": "13"})).await;
    assert_next(&mut bob, json!({"type": "vote_cast", "participant_id": 2})).await;
    assert_next(&mut ada, json!({"type": "vote_cast", "participant_id": 2})).await;

    // Errors reach the sender only; a Voter cannot reveal.
    send(&mut bob, json!({"type": "vote", "value": "4"})).await;
    assert_next(&mut bob, json!({"type": "error", "code": "invalid_value"})).await;
    send(&mut bob, json!({"type": "reveal"})).await;
    assert_next(
        &mut bob,
        json!({"type": "error", "code": "not_facilitator"}),
    )
    .await;

    send(&mut ada, json!({"type": "reveal"})).await;
    let expected = json!({"type": "votes_revealed", "item_id": 1, "votes": {"1": "5", "2": "13"}});
    assert_next(&mut ada, expected.clone()).await;
    assert_next(&mut bob, expected).await;
}
