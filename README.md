# LCARS

Stream Deck console for coding agents, backed by Herdr. The full design, decisions and build plan live in [docs/HANDOFF.md](docs/HANDOFF.md). Agent rules for this repo are in [CLAUDE.md](CLAUDE.md).

| Package | What | Stack |
| --- | --- | --- |
| `packages/ops` | Ops, the daemon. One Herdr socket per host, merged into one session list, serves the plugin | Rust (cargo workspace) |
| `packages/plugin` | LCARS, the Stream Deck plugin. Paints keys, dials and the touch strip; forwards presses to Ops | TypeScript, `@elgato/streamdeck` |

Recon notes on the Herdr socket API, with recorded frames as test fixtures: [docs/herdr-api.md](docs/herdr-api.md).

## Build

```sh
cd packages/ops && cargo build && cargo test
cd packages/plugin && npm ci && npm run typecheck && npm test
```

## Run Ops

```sh
cd packages/ops
mkdir -p ~/.config/ops && cargo run -q -- --print-default-config > ~/.config/ops/ops.toml
# edit hosts, then:
cargo run -q
curl -s http://127.0.0.1:7677/sessions | jq .
```

Remote hosts ride an SSH ControlMaster you open once after login (1Password holds the keys, Ops never prompts):

```sh
ssh -fNM -o ControlPath=~/.ssh/cm-ds9 -o ControlPersist=8h ds9
```

Ops logs to stdout; `RUST_LOG=ops=debug` for the subscription detail. Design record: [docs/adr/0001-ops-architecture.md](docs/adr/0001-ops-architecture.md).

## Run the plugin (development)

```sh
cd packages/plugin
npm ci && npm run build
streamdeck dev            # once: enables developer mode in the Stream Deck app
npm run link              # symlinks io.gamov.lcars.sdPlugin into the app
npm run restart           # after each build
```

Plugin logs land in `packages/plugin/io.gamov.lcars.sdPlugin/logs/`. The property inspector's **Ops address** is global and defaults to `http://127.0.0.1:7677`.

### Inbox Key

One Herdr agent pane per key, slot 1 = most urgent (blocked first, then most recently changed). Colour is the agent state (orange working, red blocked, green done, grey idle), the badge is the agent kind, the corner tag is the host when more than one host is online, the text is the pane title. Short tap focuses the pane through Ops and raises iTerm. Long press is reserved for Conn and does nothing yet.

### Deck layout

The Plus page "LCARS" (8 Inbox Keys, slots 1–8) was written with [streamdeck-mcp](https://github.com/verygoodplugins/streamdeck-mcp)'s profile manager, which edits Elgato's `ProfilesV3` files directly and must quit and relaunch the Stream Deck app around the write. Reproducible profile generation from a spec is milestone 7.
