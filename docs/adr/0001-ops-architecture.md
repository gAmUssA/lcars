# 0001 — Ops daemon architecture

- Status: accepted
- Date: 2026-10-18

## Context

LCARS needs one view of every coding agent across Studio, Mini and later hosts, on a Stream Deck. Herdr owns the agents, but its socket API is server-scoped: IDs and subscriptions belong to one server, and a status subscription needs an explicit `pane_id` (see `docs/herdr-api.md`). Existing Herdr deck plugins poll the CLI every few seconds against one host.

Options considered for the merge layer:

1. Extend the Node plugin to poll `herdr` over SSH per host. Rejected: polling, no events, Node on every path, the plugin stops being thin.
2. Rely on Herdr's `--machine` forwarding or a future cross-machine inventory. Rejected: no merged inventory exists; event forwarding through `--machine` is untested; roadmap items are not a dependency.
3. A separate daemon that holds one Herdr socket per host, subscribes to events, merges, and serves the plugin locally. Chosen.

For the daemon language and wire types, Rust with kcosr's vendored `herdr-compat` crate was chosen over Swift (Linux story) and over hand-written types (drift risk). Transport to remote hosts: SSH Unix-socket forwarding over a ControlMaster the user opens, because 1Password owns the SSH keys and a daemon cannot answer its prompts.

## Decision

- Ops is a Rust binary in `packages/ops`, a cargo workspace, crate `ops`.
- Herdr protocol types come from `herdr-compat`, pinned by git revision in `Cargo.toml`. Ops asserts `protocol == 22` from each host's `pong` and refuses the host otherwise.
- One `HostClient` per configured host: fresh connection per request (Herdr closes after each reply), one long-lived `events.subscribe` connection carrying `pane.created`, `pane.closed` and one `pane.agent_status_changed` per known pane. A new pane triggers a replacement subscription with the full pane set.
- Remote hosts are reached by forwarding the remote Unix socket to `~/.local/state/ops/<host>.sock` (`OPS_STATE_DIR` overrides) over `ssh -O forward` on a user-opened ControlMaster. Ops never holds a key and never prompts.
- Merged state is the `Fleet` model in `model.rs`; the plugin contract is `FleetView` served as JSON on loopback: `GET /sessions`, WebSocket `/ws` pushing the full view on every change, `GET /healthz`.
- Config is `~/.config/ops/ops.toml`; `ops --print-default-config` emits the template.

## Consequences

- Easier: adding a host is a config entry; the plugin stays a renderer; tests replay recorded Herdr frames without a live server.
- Harder: a user must open the SSH master after login before remote hosts appear; Ops reports the exact command when it is missing. Bumping Herdr means re-running the drift check against the live schema before moving the `herdr-compat` pin.
- Later work must honor: Ops normalises and relays only; it never re-implements Herdr CLI behaviour (status rendering, unblock, workspace switching) and never polls for state. Actions (focus, Conn, send-keys) will be added as relays to the owning host's socket, not as new logic.
