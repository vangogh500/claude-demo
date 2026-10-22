# Story Pointing

A server that lets a group estimate work items together over websockets: hidden picks, then a reveal. The only surface is the protocol; there is no shipped client.

## Language

**Room**:
One pointing session that a group joins, identified by a Join Code. It exists only while at least one Participant is connected.
_Avoid_: Session, game, table

**Join Code**:
The short code a Room is created with and joined by.
_Avoid_: Room id, invite

**Participant**:
A connected person in a Room, known by a display name. One socket is one Participant; display names need not be unique.
_Avoid_: User, member, player

**Facilitator**:
The Participant who created the Room. Adds Items, picks the active Item, triggers Reveal and re-votes. Also votes.
_Avoid_: Host, owner, admin, moderator

**Voter**:
Any Participant who is not the Facilitator.
_Avoid_: Member, guest

**Item**:
The thing being estimated. Has a title, a state (`Pending`, `Voting`, `Revealed`, `Estimated`) and, once Estimated, a Result.
_Avoid_: Story, ticket, issue, task (all collide with the issue-tracker sense)

**Active Item**:
The one Item in a Room that is currently Voting or Revealed. Votes attach only to it; a Room has at most one.
_Avoid_: Current story, open item

**Result**:
The value from the Scale the Facilitator records for an Item after Reveal. Need not equal any Vote.
_Avoid_: Estimate, final score, points

**Vote**:
One Participant's hidden pick from the Scale for the current Item.
_Avoid_: Estimate, card, point

**Reveal**:
The moment all Votes for an Item become visible to the whole Room.
_Avoid_: Flip, show

**Scale**:
The fixed set of values a Vote may take: modified Fibonacci (`0, 1, 2, 3, 5, 8, 13, 20, 40, 100`) plus `?`. The same for every Room.
_Avoid_: Deck, card set
