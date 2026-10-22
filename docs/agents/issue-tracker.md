# Issue tracker: Jira `LAB` + beads

This repo follows the user-level default in `~/.claude/docs/agents/issue-tracker.md`:
Jira for human interfacing, beads for AI task tracking. Read that file for the full
conventions; this one records only what is specific to this repo.

- **Jira project key:** `LAB` (Labrador) at https://micelabs.atlassian.net. One Epic
  per feature holds the spec; commits carry `Refs: LAB-<n>`.
- **Session mode:** repo session. There is no `repos.yaml` here, so this repo is the
  home and the only owner; the `~/src/mice-labs` workspace next door is ignored.
- **beads database:** `.beads/` in this repo. It does not exist yet; run
  `bd init --non-interactive --agents-file CLAUDE.md` (see "First time in a repo"
  in the user-level doc).
