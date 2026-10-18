---
# lcars-hxs6
title: 'Ops: send_keys relay'
status: completed
type: task
priority: normal
created_at: 2026-10-18T16:39:23Z
updated_at: 2026-10-18T16:45:36Z
parent: lcars-a35u
---

POST /actions send_keys {session, keys[]} -> Herdr agent.send_keys (or pane.send_keys for shells). Ops stays policy-free; the plugin decides enter/esc.

## Summary of Changes

POST /actions send_keys {session, keys[]} → agent.send_keys or pane.send_keys. Tests cover the relay, empty keys (502) and missing keys (400). Used live to confirm an AskUserQuestion dialog.
