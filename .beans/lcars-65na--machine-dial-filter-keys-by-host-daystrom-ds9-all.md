---
# lcars-65na
title: 'Machine dial: filter keys by host (Daystrom / DS9 / All)'
status: completed
type: feature
priority: normal
created_at: 2026-10-18T16:05:03Z
updated_at: 2026-10-18T16:13:43Z
parent: lcars-yeii
---

Dial 2. Rotating cycles the host filter; the Inbox Keys repaint to that host's sessions. Filter is plugin-side state shared by all keys. Press resets to All.

## Summary of Changes

MachineDial (io.gamov.lcars.dial.machine): rotate cycles all → daystrom → ds9 → all via store.cycleHostFilter; press clears. Tapping a host block on the fleet bar sets or clears the same filter. Keys and ticker both honour the filter; a filter for a vanished host falls back to all. Tests in view-state.test.ts and store.test.ts.
