---
# lcars-0h3y
title: 'Scroll-pane dial: scroll the focused pane''s terminal'
status: completed
type: feature
priority: normal
created_at: 2026-10-18T16:05:03Z
updated_at: 2026-10-18T16:13:43Z
parent: lcars-yeii
---

Dial 4 rotation -> Ops action scroll -> Herdr pane.scroll on the focused session. Dial 4 PRESS toggles the touch strip view (fleet bar / inbox ticker) per the touch strip decision; it never sends Enter.

## Summary of Changes

PaneDial (io.gamov.lcars.dial.pane): rotate scrolls the focused pane 3 rows per tick through Ops scroll (verified live on w9:p4); in ticker mode rotate cycles blocked agents instead. Press toggles the strip mode and never sends Enter.
