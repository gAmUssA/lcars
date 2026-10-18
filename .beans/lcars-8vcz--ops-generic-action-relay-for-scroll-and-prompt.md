---
# lcars-8vcz
title: 'Ops: generic action relay for scroll and prompt'
status: completed
type: task
priority: normal
created_at: 2026-10-18T16:05:03Z
updated_at: 2026-10-18T16:07:18Z
parent: lcars-yeii
---

Extend POST /actions with scroll {session, delta} -> pane.scroll and prompt {session, text} -> agent.prompt; keep Ops free of policy.

## Summary of Changes

- POST /actions now accepts `scroll {session, delta}` (pane.get, clamp, pane.scroll; returns offset_from_bottom) and `prompt {session, text}` (agent.prompt, no wait; refuses empty text and agentless panes).
- Tests in server.rs cover clamping both ways, missing delta/text, and the prompt payload. Live check on Studio: scroll up 5 rows and back to 0 on this session's pane.
