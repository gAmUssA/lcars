---
# lcars-p262
title: 'Effort dial: Claude Code reasoning effort for the focused pane'
status: completed
type: feature
priority: normal
created_at: 2026-10-18T16:05:03Z
updated_at: 2026-10-18T16:13:43Z
parent: lcars-yeii
---

Dial 3. Needs study of how linus-amg/streamdeck-claude-code injects effort; relay through a new Ops action (agent.prompt / send_text). Capability-flagged: inert for non-Claude panes.

## Summary of Changes

EffortDial (io.gamov.lcars.dial.effort): rotate picks low/medium/high/xhigh/max (linus-amg's /effort levels), press sends `/effort <level>` through Ops prompt → Herdr agent.prompt to the focused pane when it is a Claude pane, else shows an alert. Not yet exercised on a live Claude session (would type into Vik's terminal); relay path is covered by Ops tests.
