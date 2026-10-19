# Changelog

## 0.1.1 — 2026-10-19

- One version everywhere: Ops crate, plugin package and Stream Deck manifest (previously `0.4.0.0`) now carry the release number; `scripts/check-versions.sh` enforces it in `make test` and the release workflow refuses a tag that disagrees.
- Agent rules moved to `AGENTS.md` (`CLAUDE.md` imports it) and updated: Herdr 0.9.4 observed, token-protected Ops API, release steps.
- README refreshed: install from a GitHub Release, supported-agent table, 15-key MK.2, Conn and the blocked question documented where the Inbox Key is; stale "does nothing yet" and "milestone 7" notes removed.
- `lcars` skill: Ops calls need the bearer token, Workspace Key action listed, MK.2 model id for streamdeck-mcp.

## 0.1.0 — 2026-10-19

First working console: Ops daemon plus Stream Deck plugin, running on a Stream Deck +, an XL and a 15-key MK.2.

### Ops (Rust daemon)

- One Herdr socket connection per host, local or forwarded over a user-opened SSH ControlMaster; protocol 22 checked on every connect.
- Event-driven: `events.subscribe` per pane with a fresh snapshot after every re-subscribe; follow-up reads for titles, completion sequence and blocked prompts run off the event loop; a 30 s health ping catches a stalled forward.
- Merged `FleetView` (hosts, sessions, workspaces) over loopback `GET /sessions` and a WebSocket that delivers every change; bearer token in a 0600 file, loopback-only by default.
- Action relay: focus, scroll, prompt, send_keys (confirming keys only to blocked panes), focus_workspace; actions on a disconnected host are refused.
- `done` derived from Herdr's `completion_seq` for unfocused finished turns; cleared when the pane is focused.
- Install script, launchd template, Makefile targets; `make install` never loads the agent by itself.

### Plugin (Stream Deck)

- Inbox Key: one agent pane per key, paged (Plus) or wall (XL) layout, agent logo, state colour, host tag, title or the blocked agent's question; short tap focuses, long press (Conn) confirms a blocked dialog.
- Four Plus dials sharing one touch-strip scene: page, host filter, Claude effort level, pane scroll; fleet bar and inbox ticker views.
- Workspace Key, Attach (herdr / mosh / tmux), Stop All with two-press confirm, Dictate, Mute Bell.
- Done ping: key blink plus system sound on blocked and finished turns, with a per-host baseline so reconnects stay quiet.

### Tooling

- CI on Ubuntu and macOS with the installer dry run and both artifacts; release workflow on `v*` tags; Dependabot.
- Deck pages authored through streamdeck-mcp's profile manager; the `lcars` Claude Code skill documents the recipe.
- Recorded Herdr fixtures (synthetic and real Claude Code flows) drive the tests; an adversarial Grok review and its verification live in `docs/reviews/`.
