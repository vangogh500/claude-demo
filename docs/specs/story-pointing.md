# Story pointing server

Vocabulary: `GLOSSARY.md`. Architecture: ADR 0001 (`docs/adr/0001-room-is-a-transport-free-state-machine.md`).

## Problem Statement

A team that estimates work together needs everyone to pick a number independently and then compare, so that nobody anchors on the first number said aloud. Doing this over a call means someone types picks into a chat or spreadsheet, picks leak early, late joiners lose track of what is being estimated, and the person running the session has no clean way to reset a round or record what was agreed.

## Solution

A server that hosts pointing Rooms over websockets. A Facilitator creates a Room and shares its Join Code; Participants join by name. The Facilitator adds Items and activates one at a time; everyone Votes from a fixed Scale, seeing who has voted but not what; the Facilitator Reveals, optionally calls a re-vote, and records a Result. Every Participant receives the same stream of events, and a late joiner receives a full snapshot on arrival. There is no client in this version: the protocol is the product, and any websocket client (including `websocat`) can drive it.

## User Stories

### Rooms and joining

1. As a Facilitator, I want to create a Room with my display name, so that I can host an estimation session.
2. As a Facilitator, I want the server to give me a short, readable Join Code, so that I can say it aloud on a call.
3. As a Voter, I want to join a Room with a Join Code and my display name, so that I can take part without an account.
4. As a Voter, I want a clear error when the Join Code does not match a live Room, so that I know to ask for it again.
5. As a Participant, I want display names not to be required to be unique, so that two people with the same name can both join.
6. As a Participant, I want to be told the Room has ended when the last other Participant has left and I disconnect, so that nothing lingers on the server.
7. As an operator, I want many Rooms to be hosted by one process, so that one deployment serves every team.
8. As an operator, I want nothing persisted, so that a restart leaves no state to clean up.

### Snapshot and events

9. As a Participant, I want a full snapshot of the Room when I join, so that I know who is here, which Items exist, which is Active, and who has already Voted.
10. As a Participant, I want every change to the Room delivered as an event, so that I can keep my view current without polling.
11. As a Participant, I want the event caused by my own command delivered to me the same way as to everyone else, so that my client has one code path.
12. As a Participant, I want to see when someone joins or leaves, so that I know who is in the Room.
13. As a Participant, I want to be told when the Facilitator changes, so that I know who can Reveal.

### Items

14. As a Facilitator, I want to add an Item by title, so that the Room knows what we are estimating.
15. As a Facilitator, I want to add Items before anyone else has joined, so that I can prepare the session.
16. As a Facilitator, I want to activate an Item, so that Votes attach to it.
17. As a Facilitator, I want to activate a different Item while one is Active, so that I can skip an Item and come back to it later.
18. As a Participant, I want a skipped Item to return to Pending with its Votes discarded, so that stale picks never surface later.
19. As a Participant, I want Items to keep the order they were added in, so that the session follows the backlog.
20. As a Voter, I want to be refused when I try to add or activate an Item, so that only the Facilitator steers the session.

### Voting

21. As a Participant, I want to Vote on the Active Item with a value from the Scale, so that my estimate counts.
22. As a Participant, I want to change my Vote until Reveal, so that I can reconsider.
23. As a Participant, I want to see who has voted but not what, so that I am not anchored by others' picks.
24. As a Participant, I want to be refused when I Vote with a value outside the Scale, so that only valid picks are recorded.
25. As a Participant, I want to be refused when I Vote with no Active Item, so that I know the Facilitator has not started a round.
26. As a Participant, I want to be refused when I Vote on a Revealed Item, so that nothing changes what everyone has already seen.
27. As a late joiner, I want to Vote on an Item that is still Voting, so that joining late does not exclude me.
28. As a late joiner, I want to be counted in "who has voted", so that the Facilitator knows to wait for me.
29. As a Facilitator, I want to Vote too, so that my estimate counts like anyone else's.

### Reveal, re-vote, Result

30. As a Facilitator, I want to Reveal when at least one Vote exists, so that the Room can compare picks.
31. As a Facilitator, I want to be refused when I Reveal with no Votes, so that an empty Reveal is impossible.
32. As a Facilitator, I want to Reveal before everyone has voted, so that one absent Participant cannot stall the session.
33. As a Participant, I want to see every Vote with its Participant on Reveal, so that we can discuss the spread.
34. As a Facilitator, I want to call a re-vote after Reveal, so that the Room can converge after discussion.
35. As a Participant, I want a re-vote to clear all Votes and return the Item to Voting, so that the next round starts clean.
36. As a Facilitator, I want to accept a value from the Scale as the Item's Result after Reveal, so that the agreed number is recorded.
37. As a Facilitator, I want the Result not to be constrained to one of the Votes, so that a negotiated number can be recorded.
38. As a Facilitator, I want to be refused when I accept a Result before Reveal, so that nothing is recorded without discussion.
39. As a Participant, I want an Estimated Item to show its Result in the snapshot, so that a late joiner sees what was decided.
40. As a Voter, I want to be refused when I try to Reveal, re-vote or accept, so that only the Facilitator controls the round.

### Disconnection

41. As a Participant, I want a Voter who leaves to take their Vote with them, so that "who has voted" never counts someone who is gone.
42. As a Participant, I want the earliest-joined remaining Participant to become Facilitator when the Facilitator leaves, so that a dropped connection does not end the session.
43. As a newly promoted Facilitator, I want to be told I am now the Facilitator, so that I know I can Reveal.
44. As a Participant, I want the Active Item and its remaining Votes to survive a Facilitator change, so that the round continues.
45. As a Participant, I want the Room to end when its last Participant leaves, so that Join Codes are not held forever.

### Protocol hygiene

46. As a client author, I want every message to carry a `type` discriminator, so that I can dispatch without guessing.
47. As a client author, I want errors to carry a stable machine-readable code, so that I can handle them without parsing prose.
48. As a client author, I want errors delivered only to the socket that caused them, so that other Participants are not disturbed.
49. As a client author, I want to be refused when I send a command before creating or joining a Room, so that the lifecycle is unambiguous.
50. As a client author, I want to be refused when I create or join a second Room on the same socket, so that one socket is exactly one Participant.
51. As a client author, I want to be refused on a malformed frame with an error rather than a dropped connection, so that I can correct and continue.
52. As a developer, I want to drive the server with `websocat` for a demo, so that no client code is needed to show it working.

## Implementation Decisions

### Language and runtime

- Rust, `tokio` runtime, `axum` for the HTTP server and websocket upgrade. One binary crate.
- One endpoint: `GET /ws`. Bind address from a `--bind` flag or `BIND` environment variable, default `127.0.0.1:8080`. Nothing else is served.
- All state in memory; many Rooms per process; nothing survives restart.

### Room as a transport-free state machine (ADR 0001)

- The Room module owns Participants, Items, Votes, the Active Item, the Facilitator, and every transition and rejection. It knows nothing about sockets or JSON.
- Its interface is a single operation: apply a command, given the acting `participant_id`, and receive either the list of events to broadcast or an error code. Joining and leaving are commands like any other, so promotion on Facilitator departure and Vote discard on Voter departure are Room transitions, not transport side effects.
- The Room reports when it has become empty so the owning registry can drop it and release its Join Code.
- The websocket layer decodes frames into commands, holds the socket's `participant_id` and Room after the first message, fans out broadcast events to every socket in the Room, and sends errors to the originating socket only.

### Domain rules

- A Room is created by one Participant who becomes the Facilitator. Everyone else is a Voter. The Facilitator also Votes.
- A Participant is a socket. No auth, no reconnect identity; a reconnecting person is a new Participant. Display names are free text and need not be unique.
- Participant ids and Item ids are small integers assigned by the Room, unique within it, increasing in creation order.
- Join Code: four characters from an unambiguous uppercase alphabet (no `0/O`, `1/I`), unique among live Rooms, released when the Room ends.
- Scale is fixed: `0, 1, 2, 3, 5, 8, 13, 20, 40, 100, ?`. Vote and Result values are transmitted as strings.
- Item states: `Pending`, `Voting`, `Revealed`, `Estimated`. At most one Item is Active (Voting or Revealed). Items are kept in insertion order.
- Transitions:
  - `activate_item` on a Pending Item makes it Voting. If another Item was Active, it returns to Pending and its Votes are discarded; one `item_activated` event covers both.
  - `activate_item` on an Estimated Item is refused.
  - `vote` is accepted only on a Voting Item and replaces the caller's prior Vote.
  - `reveal` requires a Voting Item with at least one Vote; makes it Revealed.
  - `revote` requires a Revealed Item; clears Votes; makes it Voting.
  - `accept` requires a Revealed Item and a value from the Scale; records it as the Result; makes it Estimated; the Room then has no Active Item.
- When a Participant leaves, their Vote on the Active Item is discarded. If they were the Facilitator, the remaining Participant with the lowest id is promoted. If no Participant remains, the Room ends.

### Wire protocol

- Text frames, one JSON object per frame, `type` field as discriminator, all other fields at the top level.
- A socket's first message must be `create_room` or `join_room`. Any other command before that, or a second `create_room`/`join_room`, is an error.
- When a Room ends, the server closes any socket still attached to it.

Commands, client to server:

| `type` | Who | Fields |
|---|---|---|
| `create_room` | anyone, first message | `name` |
| `join_room` | anyone, first message | `join_code`, `name` |
| `add_item` | Facilitator | `title` |
| `activate_item` | Facilitator | `item_id` |
| `vote` | any Participant | `value` |
| `reveal` | Facilitator | — |
| `revote` | Facilitator | — |
| `accept` | Facilitator | `value` |

Events, server to client:

| `type` | Audience | Fields |
|---|---|---|
| `room_snapshot` | the joining socket | `join_code`, `you` (your `participant_id`), `facilitator_id`, `participants` (id, name), `items` (id, title, state, result), `active_item_id`, `voted` (participant ids with a Vote on the Active Item) |
| `participant_joined` | Room | `participant_id`, `name` |
| `participant_left` | Room | `participant_id` |
| `facilitator_changed` | Room | `participant_id` |
| `item_added` | Room | `item_id`, `title` |
| `item_activated` | Room | `item_id` |
| `vote_cast` | Room | `participant_id` |
| `votes_revealed` | Room | `item_id`, `votes` (participant id to value) |
| `votes_cleared` | Room | `item_id` |
| `item_estimated` | Room | `item_id`, `result` |
| `error` | the sender | `code` |

- `room_snapshot` is sent to the creator too, so creating and joining look identical to a client after the first frame.
- Every state change produces exactly one broadcast event, delivered to the acting socket as well, so a client can rebuild the snapshot from events alone.

Error codes, stable strings: `malformed`, `not_in_room`, `already_in_room`, `unknown_join_code`, `not_facilitator`, `unknown_item`, `no_active_item`, `item_not_voting`, `item_not_revealed`, `item_already_estimated`, `invalid_value`, `no_votes`.

## Testing Decisions

A good test drives the system through a seam a real caller uses and asserts only on what that caller can observe. Tests never reach into Room internals, never assert on handler structure, and never test the codec or routing in isolation.

Two seams, and only two:

- **Protocol seam** (highest): tests start the server on an ephemeral port, open real websocket clients, send JSON commands and assert on the JSON frames each client receives. These cover the happy path end to end (create, join, add, activate, vote, reveal, accept), snapshot contents for a late joiner, error delivery to the sender only, the first-message rule, and socket closure when a Room ends. Few in number; each one proves the transport layer honours the Room's events.
- **Room seam** (ADR 0001): tests construct a Room, apply commands as named Participants, and assert on the returned events or error codes. This is where the full transition matrix lives: every refused command per Item state and per role, Facilitator departure mid-vote and the promotion rule, Voter departure discarding a Vote, late join during Voting versus after Reveal, skipping an Active Item, re-vote clearing Votes, accept with a value no one voted. Exhaustive and fast; no sockets.

Prior art: none in this repo; it is empty. Tests follow the standard Rust layout — Room tests beside the module, protocol tests as integration tests under the crate's tests directory.

## Out of Scope

- Any client: web UI, CLI, TUI. `websocat` is the demo client.
- Authentication, accounts, or reconnecting as the same Participant.
- Persistence of Rooms, Items or Results across restarts; history or export of Results.
- Per-Room Scale selection, T-shirt sizes, custom decks.
- Observer role (a Participant who does not Vote).
- Auto-reveal when every Participant has voted.
- Editing, removing or reordering Items.
- Transferring the Facilitator role by command.
- Timers, rate limits, Room size limits, TLS termination.

## Further Notes

- The glossary deliberately avoids "story" and "ticket" for the thing being estimated, because both collide with the issue-tracker sense in this workspace. Use **Item**.
- "Estimate" is reserved for the Item state `Estimated`; a Participant's pick is a **Vote**, the recorded value a **Result**.
- Everything listed under Out of Scope was considered and deferred, not rejected; the protocol shape (string values, integer ids, one event per transition) was chosen so each can be added without changing existing messages.
