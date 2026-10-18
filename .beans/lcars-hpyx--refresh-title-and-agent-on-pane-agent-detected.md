---
# lcars-hpyx
title: Refresh title and agent on pane_agent_detected
status: completed
type: bug
priority: normal
created_at: 2026-10-18T16:26:58Z
updated_at: 2026-10-18T19:02:01Z
parent: lcars-a35u
---

A new pane shows without a title until its first status event or a reconnect snapshot: Ops ignores pane_agent_detected and does not subscribe to pane.updated. Fix: on pane_agent_detected (or first status event) run pane.get and apply the PaneInfo.

## Summary of Changes

Ops subscribes to pane.agent_detected and pane.focused. On a detection it runs pane.get and applies the PaneInfo (title, cwd, model) over the session while keeping name and seq; on release it clears the agent. Fake-server test covers the subscribe set and dispatch; model tests cover merge and release. Verified live: pane_focused events arrive through the launchd Ops.
