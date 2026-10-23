# 0003 — Herdr's names in Ops, and session filters on the deck

- Status: accepted
- Date: 2026-10-23

## Context

Ops called each configured Herdr socket a "host" and each pane a "session". Herdr calls one server a session (`herdr session list`, `--session NAME`), an SSH target a machine, and a pane a pane. The Mini runs three Herdr sessions; configured as three "hosts" they read as three computers, and "session" meant a pane in Ops and a server in Herdr.

The same machine pushed the fleet to 28 agents. The XL wall had 16 keys and no paging, so 12 agents had no key, and nothing on the deck could show one session's work on its own.

Options considered for the hierarchy: Stream Deck folders or one profile per session (static, a new session needs a new folder, a blocked agent behind a closed folder is invisible); grouping by machine (the Mini alone holds three unrelated sessions); a filter over the existing urgency-ordered wall (chosen).

For the rename: keep the wire contract and rename only prose (cheap, but the API and config keep the wrong words), or rename end to end with a versioned contract (chosen).

## Decision

- Herdr's vocabulary everywhere: machine → session → workspace → pane → agent. Ops config is `[[sessions]]` with `name`, `machine` (SSH alias, absent for this machine), `socket` and `control_path`. The old `[[hosts]]`, `ssh` and `remote_socket` keys still parse and log a warning at startup.
- The plugin contract is `FleetView` version 2: `{api: 2, sessions, panes, workspaces}` on `GET /fleet` and `/fleet/ws`; each session carries `machine`, each pane and workspace its `session`. Action bodies name `pane` instead of `session`. The version 1 routes (`/sessions`, `/ws`) are gone, so an older plugin sees no Ops rather than a shape it would misread; the plugin refuses any `api` other than 2 with a message naming the fix.
- The deck gets a Session Key per Herdr session. Tapping it filters the agent wall and the Workspace Keys to that session; a Workspace Key press narrows the agents to that workspace. One filter is shared by every deck and the Plus fleet bar.
- A filter never hides a blocked agent: blocked panes outside it stay on the wall, first.
- The wall pages by each deck's own wall size; a long press on a Session Key turns the page.

## Consequences

- Easier: the config, API, logs and deck say what Herdr says; adding a named session is one `[[sessions]]` entry; a session with many agents can be looked at on its own, and every agent is reachable on any deck.
- Harder: Ops and the plugin must be upgraded together (same release); deck pages written before this change keep their Inbox and Workspace Keys but need Session Keys added. The Session Dial keeps the action id `io.gamov.lcars.dial.machine` so placed dials survive.
- Later work must honor: no "host" for a Herdr session and no "session" for a pane, in code or docs; filters are view state in the plugin, Ops always serves the whole fleet; any filter or grouping keeps blocked agents visible; a `FleetView` shape change bumps `api`.
- Amends the plugin contract described in ADR 0001 (`GET /sessions`, `hosts`), which otherwise stands.
