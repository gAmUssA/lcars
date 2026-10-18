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
