---
# lcars-hpyx
title: Refresh title and agent on pane_agent_detected
status: todo
type: bug
priority: normal
created_at: 2026-10-18T16:26:58Z
updated_at: 2026-10-18T16:26:58Z
parent: lcars-a35u
---

A new pane shows without a title until its first status event or a reconnect snapshot: Ops ignores pane_agent_detected and does not subscribe to pane.updated. Fix: on pane_agent_detected (or first status event) run pane.get and apply the PaneInfo.
