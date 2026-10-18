# LCARS

Stream Deck console for coding agents, backed by [Herdr](https://herdr.dev). The full design, decisions and build plan live in [docs/HANDOFF.md](docs/HANDOFF.md). Agent rules for this repo are in [CLAUDE.md](CLAUDE.md).

| Package | What | Stack |
| --- | --- | --- |
| `packages/ops` | Ops, the daemon. One Herdr socket per host, merged into one session list, serves the plugin | Rust (cargo workspace) |
| `packages/plugin` | LCARS, the Stream Deck plugin. Paints keys, dials and the touch strip; forwards presses to Ops | TypeScript, `@elgato/streamdeck` |

Recon notes on the Herdr socket API, with recorded frames as test fixtures: [docs/herdr-api.md](docs/herdr-api.md).

## Screenshots

Stream Deck XL, agent wall page: one key per agent across both hosts (host tag top-right, agent logo, state colour, pane title).

![Stream Deck app showing the XL LCARS page](docs/images/stream-deck-xl-wall.png)

## Status

| Milestone | State |
| --- | --- |
| 1. Recon of the Herdr socket API | done |
| 2. Ops v0: per-host event subscriptions, merged `FleetView`, loopback HTTP + WebSocket | done |
| 3. Plugin v0: Inbox Key on the Stream Deck +, tap focuses the pane | done |
| 4. Plus dials and touch strip | done |
| 5. Conn (long-press confirm), done ping | done; Claude Code hooks pending a yes |
| 6. Remote and global keys | done on the XL; travel profile waits for the small deck |
| 7. XL status wall | agent wall done, global row and third-party rows planned |
| 8. Packaging, launchd, install | done (agent load and skill link are explicit steps) |

## Requirements

- macOS, Stream Deck app 7.1+ (developed on 7.6), a Stream Deck + or XL
- [Herdr](https://herdr.dev) 0.9.x on every host (protocol 22)
- Rust 1.99+ (`cargo`), Node 20.5+, `npm i -g @elgato/cli` for linking the plugin
- Remote hosts: SSH alias in `~/.ssh/config`, reachable over Tailscale

## Install

From a fresh clone on the Mac the decks are plugged into:

```sh
make build            # cargo build --release + npm ci + plugin bundle
make install          # ~/.local/bin/ops, ~/.config/ops/ops.toml (if absent), launchd plist (not loaded)
$EDITOR ~/.config/ops/ops.toml   # one [[hosts]] block per machine
ssh -fNM -o ControlPath=~/.ssh/cm-<alias> -o ControlPersist=8h <alias>   # once per remote host, after login
make start            # loads io.gamov.lcars.ops: runs now and at every login
make pack             # packages/plugin/dist/io.gamov.lcars.streamDeckPlugin, double-click to install
```

`make status`, `make stop`, `make restart`, `make uninstall` manage the agent; Ops logs to `~/Library/Logs/lcars/ops.log`. `make install-skill` links the `lcars` Claude Code skill into `~/.claude/skills` so an agent can re-author deck pages later; see [docs/skill-install.md](docs/skill-install.md).

Prebuilt artifacts: every push to `main` uploads `ops-macos-arm64` and `io.gamov.lcars.streamDeckPlugin` as workflow artifacts (Actions › CI), and a `vX.Y.Z` tag publishes them as a GitHub Release with checksums.

Then lay out the decks in the Stream Deck app from the **LCARS** category, or let the skill write pages for you. macOS asks once for Automation consent the first time a key drives iTerm2.

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
ssh -fNM -o ControlPath=~/.ssh/cm-mini -o ControlPersist=8h mini
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

### Conn and the done ping

- **Conn** is a long press (400 ms or more) on an Inbox Key whose agent is `blocked`: Ops presses Enter in that pane, which confirms whatever the agent's dialog has highlighted. A short tap never does this; a long press on a non-blocked key only blinks.
- **Done ping**: when an agent enters `blocked` or finishes a turn (`working` → `idle`), its key blinks and a system sound plays (Ping for blocked, Glass for done). The first view after a reconnect only primes the baseline. Mute with the **Bell** setting on any Inbox Key.

### Global keys

| Key | Press |
| --- | --- |
| Attach | opens an iTerm2 window running `herdr` locally, `ssh -t <alias> herdr`, `mosh <alias> -- herdr` or `ssh -t <alias> tmux attach` (host and mode per key) |
| Stop All | first press arms (SURE? with the busy count, 4 s); second press sends Escape to every working or blocked agent through Ops |
| Dictate | raises iTerm2 and clicks Edit › Start Dictation |
| Mute Bell | toggles the done-ping sound |

macOS asks once for Automation consent ("Stream Deck wants access to control iTerm" and "System Events") the first time Attach, Dictate or a focusing tap runs. Until you click Allow those keys show an alert.

### Dials and touch strip (Stream Deck +)

| Dial | Rotate | Press | Strip segment |
| --- | --- | --- | --- |
| 1 Scroll | page the Inbox Keys (8 per page) | back to page 1 | fleet bar |
| 2 Machine | cycle host filter: all → each host | show all hosts | fleet bar |
| 3 Effort | pick a Claude Code effort level (low … max) | send `/effort <level>` to the focused Claude pane | fleet bar |
| 4 Pane | scroll the focused pane's terminal (3 rows per tick); in ticker mode, cycle blocked agents | toggle fleet bar / inbox ticker | fleet bar |

The strip is one 800×100 scene sliced per dial. Fleet bar: one block per host coloured by its worst agent state with counts, tap a block to filter the keys to that host (tap again to clear); spare blocks show page and effort. Inbox ticker: the oldest blocked agent with its host and title, tap to focus it. The strip dims when Ops is unreachable.

### XL agent wall

The XL page "LCARS" holds 24 Inbox Keys in **wall** layout: slot n is the n-th agent across all hosts in Ops's order (blocked first, then most recent), no paging. Row 4 is free for the global keys of milestone 7. New agents appear as soon as Ops sees their first status event; keys beyond the agent count stay black.

### Deck layout

The Plus page "LCARS" (8 Inbox Keys, slots 1–8, plus the four dials) was written with [streamdeck-mcp](https://github.com/verygoodplugins/streamdeck-mcp)'s profile manager, which edits Elgato's `ProfilesV3` files directly and must quit and relaunch the Stream Deck app around the write. Reproducible profile generation from a spec is milestone 7.

## License

[MIT](LICENSE). Third-party notices for the plugin are in [packages/plugin/THIRD_PARTY_NOTICES.md](packages/plugin/THIRD_PARTY_NOTICES.md).
