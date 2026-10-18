---
# lcars-zall
title: 'Scroll dial: page the pane list when more than 8 panes'
status: completed
type: feature
priority: normal
created_at: 2026-10-18T16:05:03Z
updated_at: 2026-10-18T16:13:43Z
parent: lcars-yeii
---

Dial 1. Rotating moves a window over the keyed sessions; keys show the window; the dial's strip segment shows position (e.g. 1-8 of 16). Press returns to the top.

## Summary of Changes

ScrollDial (io.gamov.lcars.dial.scroll): rotate moves the 8-key window over the filtered session list, press returns to page 1; the fleet bar shows PAGE n/m in a spare segment. Tests cover clamping and reset.
