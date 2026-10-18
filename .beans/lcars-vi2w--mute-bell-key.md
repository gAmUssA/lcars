---
# lcars-vi2w
title: Mute Bell key
status: completed
type: feature
priority: normal
created_at: 2026-10-18T16:51:07Z
updated_at: 2026-10-18T16:58:23Z
parent: lcars-0jge
---

Toggles the global bell setting from a key and shows the state.

## Summary of Changes

MuteBellAction toggles the global bell setting and repaints BELL/MUTED; plugin.ts reads the setting on every global-settings event.
