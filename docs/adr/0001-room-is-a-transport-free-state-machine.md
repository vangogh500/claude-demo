# Room is a transport-free state machine

The Room — its Participants, Items, Votes, every transition and every rejection — is a
pure module that knows nothing about sockets or JSON. It takes a command plus the acting
`participant_id` and returns the events to broadcast (or the error to reply with); the
websocket layer only decodes, routes, and fans out. We chose this over handling state
inside the connection handlers because the interesting behaviour is all in the edge cases
(Facilitator disconnects mid-vote, late joiner after Reveal, Voter leaves after voting)
and those need exhaustive unit tests that do not open a socket. A small set of
integration tests drives real websocket clients against an ephemeral port to cover the
transport layer itself.
