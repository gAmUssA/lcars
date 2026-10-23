---
# lcars-9jvr
title: Session hierarchy on the deck and Herdr-consistent naming
status: completed
type: feature
priority: normal
created_at: 2026-10-23T18:37:56Z
updated_at: 2026-10-23T19:01:10Z
---

Ops called a Herdr server a "host" and an agent pane a "session"; Herdr calls the server a session (`herdr session list`, `--session`) and the SSH target a machine. DS9 runs three Herdr sessions, which made the mismatch visible, and the XL wall (16 slots, no paging) hid 12 of 28 agents.

## Checklist

- [x] Ops: host -> session, session -> pane in model, server, relay, banners, client
- [x] Config: `[[sessions]]` with `machine` + `socket`; old `[[hosts]]`/`ssh`/`remote_socket` still parse with a startup warning
- [x] API: `FleetView` api 2 on `/fleet` and `/fleet/ws`; old routes 404; `machine` on each session
- [x] Plugin: same rename, parser requires api 2
- [x] Session Key: one Herdr session per key, worst-state colour and counts, tap filters, long press pages the wall
- [x] Workspace filter: under a session filter the Workspace Keys show that session's workspaces; tap narrows the agents to it
- [x] Blocked agents from outside the filter stay pinned first
- [x] Wall paging on the XL and MK.2
- [x] Manifest, property inspectors, README, AGENTS.md, skill, ops.toml.example, CHANGELOG, ADR 0003
- [x] Migrate the operator's ops.toml, install, verify on the deck


## Summary of Changes

- Ops speaks Herdr's words: sessions (Herdr servers) and panes, `[[sessions]]` config with `machine` and `socket`, legacy keys still parse with a warning.
- Plugin API v2 on `/fleet` and `/fleet/ws` with `api: 2`; v1 routes return 404 and the plugin refuses other versions.
- Session Key (tap filters wall and Workspace Keys, long press pages the wall), Workspace Key narrows to a workspace, blocked agents stay pinned, per-deck wall paging.
- XL page row 1 = 4 Session + 4 Workspace Keys, 14-key wall; MK.2 travel page got 3 Session Keys. Operator's ops.toml migrated, Ops and plugin installed from main, decks verified by screenshot.
- ADR 0003, README, AGENTS.md, skill, herdr-api notes, CHANGELOG. PR #2, CI green.
