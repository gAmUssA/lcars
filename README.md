# LCARS

Stream Deck console for coding agents, backed by [Herdr](https://herdr.dev). The full design, decisions and build plan live in [docs/HANDOFF.md](docs/HANDOFF.md). Agent rules for this repo are in [CLAUDE.md](CLAUDE.md).

| Package | What | Stack |
| --- | --- | --- |
| `packages/ops` | Ops, the daemon. One Herdr socket per host, merged into one session list, serves the plugin | Rust (cargo workspace) |
| `packages/plugin` | LCARS, the Stream Deck plugin. Paints keys, dials and the touch strip; forwards presses to Ops | TypeScript, `@elgato/streamdeck` |

Recon notes on the Herdr socket API, with recorded frames as test fixtures: [docs/herdr-api.md](docs/herdr-api.md).

## Status

| Milestone | State |
| --- | --- |
| 1. Recon of the Herdr socket API | done |
| 2. Ops v0: per-host event subscriptions, merged `FleetView`, loopback HTTP + WebSocket | done |
| 3. Plugin v0: Inbox Key on the Stream Deck +, tap focuses the pane | done |
| 4. Plus dials and touch strip | done |
| 5. Claude Code hooks, Conn (long-press approve), done ping | next |
| 6. Remote keys and travel profile | planned |
| 7. XL status wall | planned |
| 8. Packaging, launchd, install | planned |

## Requirements

- macOS, Stream Deck app 7.1+ (developed on 7.6), a Stream Deck + or XL
- [Herdr](https://herdr.dev) 0.9.x on every host (protocol 22)
- Rust 1.99+ (`cargo`), Node 20.5+, `npm i -g @elgato/cli` for linking the plugin
- Remote hosts: SSH alias in `~/.ssh/config`, reachable over Tailscale

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

### Dials and touch strip (Stream Deck +)

| Dial | Rotate | Press | Strip segment |
| --- | --- | --- | --- |
| 1 Scroll | page the Inbox Keys (8 per page) | back to page 1 | fleet bar |
| 2 Machine | cycle host filter: all → each host | show all hosts | fleet bar |
| 3 Effort | pick a Claude Code effort level (low … max) | send `/effort <level>` to the focused Claude pane | fleet bar |
| 4 Pane | scroll the focused pane's terminal (3 rows per tick); in ticker mode, cycle blocked agents | toggle fleet bar / inbox ticker | fleet bar |

The strip is one 800×100 scene sliced per dial. Fleet bar: one block per host coloured by its worst agent state with counts, tap a block to filter the keys to that host (tap again to clear); spare blocks show page and effort. Inbox ticker: the oldest blocked agent with its host and title, tap to focus it. The strip dims when Ops is unreachable.

### Deck layout

The Plus page "LCARS" (8 Inbox Keys, slots 1–8, plus the four dials) was written with [streamdeck-mcp](https://github.com/verygoodplugins/streamdeck-mcp)'s profile manager, which edits Elgato's `ProfilesV3` files directly and must quit and relaunch the Stream Deck app around the write. Reproducible profile generation from a spec is milestone 7.

## License

[MIT](LICENSE). Third-party notices for the plugin are in [packages/plugin/THIRD_PARTY_NOTICES.md](packages/plugin/THIRD_PARTY_NOTICES.md).
