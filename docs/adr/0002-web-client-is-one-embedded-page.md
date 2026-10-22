# The Web Client is one embedded page with no build step

The Web Client is a single hand-written HTML file with inline JavaScript and CSS. It is
compiled into the server binary with `include_str!` and served at `GET /`. It connects to
`/ws` on the origin it was loaded from. We chose this over a framework with a Node
toolchain, and over a separately deployed static site, for three reasons. The protocol is
small and its logic is already exhaustively tested at the Room seam (ADR 0001), so the
client is a thin renderer. One binary keeps deployment to one artifact with no CORS or
server-URL configuration. The repo stays on a single Rust toolchain.

## Consequences

- The Web Client is built strictly against the wire protocol as specified. A gap it
  exposes becomes a protocol change in its own right, never a client-side workaround.
- A page reload makes the browser a new Participant, because the server has no
  reconnect identity. The Web Client shows the disconnection and does not mask it.
- Client state handling is covered by a smoke checklist, not automated browser tests.
  If that state handling grows, the first step is to extract a pure event reducer and
  unit test it, which reopens the toolchain question.
