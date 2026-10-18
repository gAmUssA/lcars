# LCARS

Stream Deck console for coding agents. Read `docs/HANDOFF.md` before changing architecture.

## Naming

- Project and Stream Deck plugin: LCARS. Repo layout: `packages/ops`, `packages/plugin`.
- Rust daemon: Ops. Binary `ops`, config `ops.toml`, local socket `ops.sock`.
- Long-press unblock/approve action: Conn.
- Hosts: Daystrom (Mac Studio, deck host), DS9 (Mac Mini), Voyager (MacBook, travel), Arch Linux Mini. All on Tailscale MagicDNS.

## Components

- Ops: Rust, built on kcosr's `herdr-compat` crate (targets Herdr v0.9.3, protocol 22). Cargo workspace in `packages/ops`.
- Plugin: Node 20+/TypeScript, `@elgato/streamdeck` v2, fork of timvdhoorn/stream-deck-herdr-plugin. Thin: paint keys, forward presses to Ops.
- Herdr is the only agent source. Every host runs it. No other agent discovery path.

## Herdr rules

- Herdr sockets are server-scoped. Ops holds one connection per host and merges the streams itself.
- Subscribe to events (`events.subscribe`, `pane.agent_status_changed`). Never poll the CLI or socket for state.
- Ops never re-implements what Herdr CLI or the forked plugin already does. It normalises, merges, and relays actions only.
- Confirm CLI verbs and socket schema from `herdr --help` and `docs/herdr-api.md`, not from memory.

## Input rules

- Conn (unblock/approve) is long-press only. Short tap only focuses the pane.
- Claude Code-only controls (effort dial, usage rings) sit behind a capability flag.

## Hard constraints

- No cloud relay. No telemetry. No push to phone.
- No tmux state scraping. tmux is for `attach-remote` keys and last-resort `send-keys` only.
- SSH over Tailscale only. No port forwarding, no relay.
- Terminal is iTerm2. Do not switch terminals.

## Ask first

- Before writing to `~/.claude/settings.json`.
- Before installing a launchd agent.
- SSH aliases and hostnames: ask, never guess.
- Before any machine-level install (`brew`, `npm -g`, `cargo install`).

## Workflow

- Work in milestones from `docs/HANDOFF.md` Build plan. Commit at the end of each milestone, message names the milestone.
- Stop and summarise before starting the next milestone.
- Recorded Herdr socket frames in `docs/herdr-api.md` are the test fixtures for Ops.
