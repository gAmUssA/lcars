---
# lcars-6i3z
title: Derive done from completion_seq
status: completed
type: bug
priority: low
created_at: 2026-10-18T16:45:36Z
updated_at: 2026-10-18T19:02:01Z
parent: lcars-a35u
---

Events never say done; after a turn agent.get has completion_seq == state_change_seq. Ops could derive done for unfocused panes so keys turn green until focused. Needs a pane.get on working→idle or hooks.

## Summary of Changes

After any status event that reaches idle, Ops runs agent.get; derive_state() maps idle + completion_seq == state_change_seq + !focused to done. pane_focused flips done back to idle and sets the focused flag across the host. Model tests use the AgentInfo shape recorded in claude-turn-flow.json; client test asserts the AgentRefreshed follows the idle event.
