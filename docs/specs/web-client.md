# Story pointing Web Client

Vocabulary: `GLOSSARY.md`. Architecture: ADR 0001 (`docs/adr/0001-room-is-a-transport-free-state-machine.md`) and ADR 0002 (`docs/adr/0002-web-client-is-one-embedded-page.md`). Protocol: the story pointing server spec (`docs/specs/story-pointing.md`).

## Problem Statement

The pointing server works, but the only way to use it is to type JSON frames into `websocat`. A team on a call cannot do that: nobody wants to hand-write `{"type":"vote","value":"5"}`, the raw event stream is unreadable as a picture of the Room, and the Facilitator has to remember Item ids and error codes. The protocol is complete, yet the product is unusable by the people it is for.

## Solution

The server also serves a Web Client: one minimal browser page at the server's root URL. A Facilitator opens it, enters a display name, creates a Room and shares the Join Code or a share link. Voters open the link, enter a name, and join. The page shows who is in the Room, the Items, which one is Active, and who has voted; everyone taps a Scale value to Vote; the Facilitator alone sees the controls to add and activate Items, Reveal, call a re-vote and accept a Result. The Web Client drives the existing protocol exactly as specified and changes nothing on the wire. It is plain and unstyled beyond readability, and it works on a phone.

## User Stories

### Opening and entering

1. As a Participant, I want to open the server's root URL in a browser and get the Web Client, so that I need nothing installed.
2. As a Participant, I want the Web Client to connect to the same server it was loaded from, so that there is no server address to configure.
3. As a Participant on an HTTPS deployment, I want the Web Client to use a secure websocket, so that it works behind a TLS-terminating proxy.
4. As a Participant, I want to enter my display name before anything else, so that the Room knows who I am.
5. As a Facilitator, I want a "Create Room" action, so that I can start a session with my display name.
6. As a Voter, I want a "Join" action with a Join Code field, so that I can enter the code someone read out on the call.
7. As a Voter, I want the Join Code field to accept lowercase and surrounding spaces, so that a typed code is not rejected for its case.
8. As a Facilitator, I want a share link that carries the Join Code, so that I can paste one link into the call chat.
9. As a Voter, I want opening a share link to prefill the Join Code, so that I only type my name.
10. As a Voter, I want a readable message when the Join Code does not match a live Room, so that I know to ask for it again.
11. As a Participant, I want the create and join actions disabled while my display name is empty, so that I cannot send a nameless command.

### Seeing the Room

12. As a Participant, I want the Join Code shown prominently once I am in the Room, so that I can read it out to latecomers.
13. As a Participant, I want to see every Participant's display name, so that I know who is here.
14. As a Participant, I want the Facilitator marked in the Participant list, so that I know who steers the session.
15. As a Participant, I want myself marked in the Participant list, so that I can tell myself apart from someone with the same name.
16. As a Participant, I want joins and leaves reflected immediately, so that the list is never stale.
17. As a Participant, I want to see who has voted on the Active Item but not what, so that I know whom we are waiting for without being anchored.
18. As a Participant, I want the Items listed in the order they were added, so that the session follows the backlog.
19. As a Participant, I want each Item to show its state, so that I can tell Pending, Voting, Revealed and Estimated apart.
20. As a Participant, I want each Estimated Item to show its Result, so that I can see what was decided.
21. As a Participant, I want the Active Item highlighted, so that I know what we are estimating now.
22. As a late joiner, I want to see the full current Room on arrival, so that I can take part at once.

### Voting

23. As a Participant, I want the Scale shown as large buttons while the Active Item is Voting, so that I can Vote with one tap, including on a phone.
24. As a Participant, I want my own pick highlighted after I Vote, so that I know what I chose.
25. As a Participant, I want to tap a different value to change my Vote until Reveal, so that I can reconsider.
26. As a Participant, I want the Scale buttons hidden or disabled when there is no Active Item or it is Revealed, so that I cannot attempt a Vote the server would refuse.
27. As a Participant, I want my highlighted pick cleared when the Facilitator calls a re-vote, so that the new round starts clean.
28. As a Participant, I want my highlighted pick cleared when a different Item becomes Active, so that a skipped Item's pick never carries over.
29. As a Facilitator, I want the same Scale buttons as everyone else, so that my Vote counts like anyone's.

### Reveal, re-vote, Result

30. As a Participant, I want every Vote shown with its Participant's name on Reveal, so that we can discuss the spread.
31. As a Participant, I want revealed Votes sorted by value, so that the spread is visible at a glance.
32. As a Participant, I want no average or consensus figure shown, so that an ordinal Scale with `?` is not misrepresented.
33. As a Participant who joined after Reveal, I want to be told the Votes were revealed before I joined, so that an empty reveal is not mistaken for no Votes.
34. As a Participant, I want a revealed Vote removed when its Participant leaves, so that my view matches the Room, which discards a departed Participant's Vote.

### Facilitator controls

35. As a Facilitator, I want an input to add an Item by title, so that I can build the backlog.
36. As a Facilitator, I want to add Items before anyone else has joined, so that I can prepare the session.
37. As a Facilitator, I want an "Activate" control on each Pending Item, so that I can start Voting on it.
38. As a Facilitator, I want to activate another Item while one is Active, so that I can skip an Item and come back to it.
39. As a Facilitator, I want no "Activate" control on Estimated Items, so that I cannot attempt a refused command.
40. As a Facilitator, I want a "Reveal" control while the Active Item is Voting, enabled only once someone has voted, so that an empty Reveal is impossible.
41. As a Facilitator, I want a "Re-vote" control while the Active Item is Revealed, so that the Room can converge after discussion.
42. As a Facilitator, I want to accept a Result by tapping a Scale value while the Active Item is Revealed, so that the agreed number is recorded.
43. As a Facilitator, I want no Result preselected, so that I am not anchored toward any Vote.
44. As a Voter, I want none of the Facilitator controls shown to me, so that my view holds only what I can do.
45. As a newly promoted Facilitator, I want the Facilitator controls to appear as soon as I am promoted, so that I can continue the round.
46. As a newly promoted Facilitator, I want a notice that I am now the Facilitator, so that I know the controls are mine.
47. As a Participant, I want the Active Item and its Votes to stay on screen through a Facilitator change, so that the round visibly continues.

### Errors and disconnection

48. As a Participant, I want each error the server sends shown as one readable sentence, so that I do not have to know error codes.
49. As a Participant, I want an error I do not recognise shown as its raw code, so that a newer server never produces a blank message.
50. As a Participant, I want error messages to be dismissable and never to change what the page shows of the Room, so that an error cannot desynchronise my view.
51. As a Participant, I want a clear "disconnected" state when the connection closes, so that I know I am no longer in the Room.
52. As a Participant, I want the disconnected state to tell me that rejoining makes me a new Participant and drops my Vote, so that I understand what reloading costs.
53. As a Participant, I want a "Rejoin" action in the disconnected state that returns me to entry with the Join Code prefilled, so that getting back in is one step.
54. As a Participant, I want the Web Client never to reconnect silently, so that it never claims an identity the server does not keep.
55. As a Participant who rejoins after everyone else has left, I want the unknown-Join-Code message, so that I know the Room has ended and I can create a new one.

### Delivery

56. As an operator, I want the Web Client inside the server binary, so that deploying the server deploys the Web Client.
57. As an operator, I want no build step and no second toolchain, so that building the server is still one `cargo build`.
58. As a developer, I want `websocat` to keep working unchanged, so that the Web Client is one Client among others.

## Implementation Decisions

### Delivery (ADR 0002)

- The Web Client is one hand-written HTML document with inline JavaScript and inline CSS. No framework, bundler, package manager or external asset.
- It is compiled into the server binary and served at `GET /` with an HTML content type. The server's only routes become `GET /` and `GET /ws`; the server spec's "nothing else is served" is amended accordingly.
- It connects to `/ws` on the origin it was loaded from, choosing `wss:` when the page was loaded over HTTPS and `ws:` otherwise. There is no configurable server URL.

### Protocol

- The wire protocol is frozen for this feature: no new commands, events, fields or error codes. The Web Client uses all eight commands and handles all eleven events.
- Anything the Web Client wants that the protocol lacks is done client-side or deferred to its own server ticket. The one known gap, revealed Vote values missing from the snapshot, is a separate ticket; until it lands, the Web Client shows the "revealed before you joined" notice.

### Client state

- The Web Client holds one state object rebuilt from the `room_snapshot` and updated by each event, mirroring the protocol's promise that the snapshot can be rebuilt from events alone. The page re-renders from that state; no view element holds state of its own.
- The state holds: the Join Code, `you`, the Facilitator id, Participants (id, name) in join order, Items (id, title, state, Result) in insertion order, the Active Item id, the set of Participant ids that voted, the revealed Votes when present, and the caller's own last-sent Vote value.
- Event handling:
  - `participant_joined` / `participant_left` add and remove Participants; a leaver is also removed from the voted set and from the revealed Votes, matching the Room's discard of a departed Participant's Vote in every Item state.
  - `facilitator_changed` updates the Facilitator id and, when it names `you`, raises the "you are now the Facilitator" notice.
  - `item_added` appends an Item as Pending.
  - `item_activated` returns any previously Active Item to Pending, makes the named Item Voting and Active, and clears the voted set, revealed Votes and own Vote.
  - `vote_cast` adds the Participant to the voted set.
  - `votes_revealed` marks the Item Revealed and stores the Votes.
  - `votes_cleared` returns the Item to Voting and clears the voted set, revealed Votes and own Vote.
  - `item_estimated` marks the Item Estimated with its Result and clears the Active Item.
  - `error` raises a dismissable message and changes nothing else.
- The own-Vote value is the only client-held fact the server does not echo. It is set when the Web Client sends `vote`, and it is lost on reload, matching the server's discard of a departed Participant's Vote.

### Roles and controls

- Facilitator controls (add Item, Activate, Reveal, Re-vote, accept Result) render only when `you` equals the Facilitator id. The server's `not_facilitator` refusal remains the real guard.
- Controls render only in the states where the server would accept them: Activate on every Pending Item, whether or not another Item is Active (the Room accepts it and skips the old one); Reveal while the Active Item is Voting with a non-empty voted set; Re-vote and accept while it is Revealed; Scale Vote buttons while it is Voting.

### Entry and share link

- The entry screen has a display-name field, a "Create Room" action, and a Join Code field with a "Join" action. Join Codes are trimmed and uppercased before sending.
- The share link is the page URL with the Join Code as its fragment. On load, a fragment prefills the Join Code field. The fragment never reaches the server.
- Display names are not remembered between visits.

### Errors and disconnection

- Each of the twelve error codes maps to one human sentence; an unrecognised code shows as its raw text.
- When the socket closes, the Web Client shows a disconnected state explaining that rejoining creates a new Participant, with a "Rejoin" action that returns to entry with the Join Code prefilled. It never reconnects on its own.

### Presentation

- A small inline style block, no CSS framework. A viewport meta tag makes the page usable on a phone; Scale values are large tap targets.
- Revealed Votes are listed sorted by Scale order, with `?` last, each beside its Participant's name. No average, median or consensus figure.

## Testing Decisions

A good test drives the system through a seam a real caller uses and asserts only on what that caller can observe. This feature adds no new seam.

- **Protocol seam** (existing, highest): the integration tests that start the server on an ephemeral port gain one test asserting that `GET /` returns a successful HTML response containing the Web Client. Prior art is the existing websocket integration tests in the crate's tests directory, which already start the server this way.
- **Room seam** (ADR 0001): unchanged. The Web Client adds no Room behaviour.
- **No browser automation and no JavaScript test runner**, per ADR 0002. If the client's state handling grows, the first step is to extract a pure event reducer and unit test it, which reopens the toolchain question.
- **Manual smoke checklist**, run against a local server with two browser windows (one of them a phone-sized viewport) before the PR merges:
  1. Create a Room in window A; the Join Code and share link appear.
  2. Open the share link in window B; the Join Code is prefilled; join with a name.
  3. A adds two Items; B sees them in order, with no Facilitator controls.
  4. A activates the first Item; both Vote; each sees the other marked as voted but not the value; each sees its own pick highlighted.
  5. A reveals; both see the Votes sorted by value with names.
  6. A calls a re-vote; Votes and highlights clear; both Vote again; A reveals and accepts a value nobody voted; the Item shows Estimated with that Result.
  7. A activates the second Item, then closes window A; B is promoted, sees the notice and the Facilitator controls, and the Active Item survives.
  8. B reloads; the page shows the disconnected state; Rejoin is offered; nothing reconnects silently.
  9. Join with a wrong Join Code; a readable error appears.

## Out of Scope

- Any protocol change in this feature, including revealed Votes in the snapshot (a separate server ticket).
- Remembering display names, automatic reconnect, or any client-side attempt at reconnect identity.
- Sounds, notifications, timers, dark mode, theming, or a CSS framework.
- A list or export of past Results beyond the Item list itself.
- A spectator or presentation view distinct from a Participant's.
- A JavaScript build step, framework, or test runner; browser automation.
- Everything the server spec already defers: authentication, persistence, custom Scales, Observer role, auto-reveal, editing or reordering Items, transferring the Facilitator role, rate and size limits, TLS termination.

## Further Notes

- **Web Client** is the canonical term, distinct from **Client**, which is anything that speaks the protocol, `websocat` included. Avoid "UI", "frontend" and "app" in code and docs.
- The server spec is amended alongside this one: its "nothing else is served" line now names `GET /`, and "any client: web UI" leaves its Out of Scope list.
- A page reload is a new Participant by design (server spec: no reconnect identity). The Web Client surfaces this rather than hiding it, because a hidden reconnect would silently drop a Vote or promote another Facilitator while appearing seamless.
