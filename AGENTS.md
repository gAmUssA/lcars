# LCARS

Stream Deck console for coding agents. Read `docs/HANDOFF.md` before changing architecture; `docs/herdr-api.md` records what the Herdr socket actually does.

## Naming

- Project and Stream Deck plugin: LCARS. Repo layout: `packages/ops`, `packages/plugin`, `packages/skill`.
- Rust daemon: Ops. Binary `ops`, config `~/.config/ops/ops.toml`, launchd label `io.gamov.lcars.ops`, state dir `~/.local/state/ops` (bearer token lives there).
- Long-press unblock/approve action: Conn.
- Hosts in docs and config: Studio (Mac Studio, deck host), Mini (Mac Mini), Laptop (MacBook, travel), Arch Linux Mini (deferred). All on Tailscale MagicDNS. Real host names, usernames and Tailscale IPs stay out of the repo.

## Components

- Ops: Rust, built on kcosr's `herdr-compat` crate pinned by git revision (baseline Herdr 0.9.3; verified against 0.9.4 and 0.9.0, protocol 22). Cargo workspace in `packages/ops`.
- Plugin: Node 20+/TypeScript, `@elgato/streamdeck` v3, fork of timvdhoorn/stream-deck-herdr-plugin. Thin: paint keys, forward presses to Ops over HTTP + WebSocket with the token from `~/.local/state/ops/token`.
- Skill: `packages/skill/lcars`, symlinked into `~/.claude/skills` by `make install-skill`.
- Herdr is the only agent source. Every host runs it. No other agent discovery path.

## Herdr rules

- Herdr sockets are server-scoped. Ops holds one connection per host and merges the streams itself.
- Subscribe to events (`events.subscribe`; `pane.agent_status_changed` needs a `pane_id` per pane). Never poll the CLI or socket for state.
- One request per connection except the subscription; follow-up reads (`pane.get`, `agent.get`, `agent.read`) run off the event loop.
- Ops never re-implements what Herdr CLI or the forked plugin already does. It normalises, merges, and relays actions only.
- Two exceptions, both outside Herdr's domain (ADR 0002): Ops raises desktop banners (`[banners]`) and reveals the iTerm session hosting a local Herdr client after `focus`. `/healthz` carries `X-Ops-Banners: on` while banners are on, and herdr-notify defers to it.
- Confirm CLI verbs and socket schema from `herdr --help` and `docs/herdr-api.md`, not from memory. Recorded frames in `docs/fixtures/` are the Ops test fixtures.

## Input rules

- Conn (unblock/approve) is long-press only. Short tap only focuses the pane. Ops relays confirming keys only to a `blocked` pane.
- Claude Code-only controls (effort dial, usage rings) sit behind a capability flag.

## Hard constraints

- No cloud relay. No telemetry. No push to phone.
- No tmux state scraping. tmux is for Attach keys and last-resort `send-keys` only.
- SSH over Tailscale only, riding a ControlMaster the user opens. No port forwarding beyond the socket forward, no relay, no passphrase-less keys.
- Terminal is iTerm2. Do not switch terminals.
- Ops listens on loopback and requires the bearer token on every endpoint but `/healthz`.

## Ask first

- Before writing to `~/.claude/settings.json`.
- Before loading a launchd agent (`make start`) or any machine-level install (`brew`, `npm -g`, `cargo install`).
- SSH aliases and hostnames: ask, never guess. Never edit `~/.ssh/config` or other dotfiles.
- Before rewriting git history or making the repo public.

## Workflow

- Track work as beans (`.beans/`, prefix `lcars-`); one bean per task, committed with its code.
- Commit messages: imperative subject, body says why, no AI attribution trailers.
- Tests before handoff: `make test` (Rust, TypeScript, shell harnesses, version check). CI must be green after every push.
- Release: bump `packages/ops/Cargo.toml`, `packages/plugin/package.json` (+ lockfile via `npm version`), the manifest `Version` (`X.Y.Z.0`) and add a dated `## X.Y.Z` heading to `CHANGELOG.md`; `scripts/check-versions.sh vX.Y.Z` must pass; tag `vX.Y.Z` and watch the Release workflow, then verify the assets against `SHA256SUMS`.
