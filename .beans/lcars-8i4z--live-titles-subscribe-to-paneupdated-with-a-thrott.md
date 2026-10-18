---
# lcars-8i4z
title: 'Live titles: subscribe to pane.updated with a throttle'
status: todo
type: bug
priority: low
created_at: 2026-10-18T19:21:25Z
updated_at: 2026-10-18T19:21:25Z
parent: lcars-bv6p
---

Review finding 13: titles freeze during a turn because Ops has no pane.updated subscription. pane_updated fires on scroll and output too, so merge only title/cwd/agent changes and coalesce per pane.
