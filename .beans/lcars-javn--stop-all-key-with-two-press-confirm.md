---
# lcars-javn
title: Stop All key with two-press confirm
status: completed
type: feature
priority: normal
created_at: 2026-10-18T16:51:07Z
updated_at: 2026-10-18T16:58:23Z
parent: lcars-0jge
---

First press arms (key shows ARM? and the count of working/blocked agents, 4 s window); second press sends esc through Ops send_keys to each of them, which interrupts the current turn. Ops has no fan-out; the plugin loops.

## Summary of Changes

StopAllAction: arm/fire via createArmedTrigger (4 s window, tests), fires Ops send_keys esc to each working or blocked session, shows the busy count. Not fired live on purpose: it would interrupt Vik's running agents.
