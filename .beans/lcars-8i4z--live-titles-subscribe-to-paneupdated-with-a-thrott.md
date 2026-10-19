---
# lcars-8i4z
title: 'Live titles: subscribe to pane.updated with a throttle'
status: completed
type: bug
priority: low
created_at: 2026-10-18T19:21:25Z
updated_at: 2026-10-19T19:47:08Z
parent: lcars-bv6p
---

Review finding 13: titles freeze during a turn because Ops has no pane.updated subscription. pane_updated fires on scroll and output too, so merge only title/cwd/agent changes and coalesce per pane.

Review verification (Grok, 2026-10-18): #13 not fixed, as intended; #8 partially fixed: the event read has no timeout, a stalled forward is noticed by the 30 s health ping within ~40 s. Accepted.

## Summary of Changes

Ops subscribes to pane.updated and routes it through the existing PaneRefreshed merge, which compares the session fields the deck shows and ignores scroll/output-only frames, so no separate throttle was needed. Titles and cwd now move during a turn.
