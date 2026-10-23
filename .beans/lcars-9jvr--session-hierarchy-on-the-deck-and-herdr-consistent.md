---
# lcars-9jvr
title: Session hierarchy on the deck and Herdr-consistent naming
status: in-progress
type: feature
priority: normal
created_at: 2026-10-23T18:37:56Z
updated_at: 2026-10-23T18:44:58Z
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
- [ ] Manifest, property inspectors, README, AGENTS.md, skill, ops.toml.example, CHANGELOG, ADR 0003
- [ ] Migrate the operator's ops.toml, install, verify on the deck
