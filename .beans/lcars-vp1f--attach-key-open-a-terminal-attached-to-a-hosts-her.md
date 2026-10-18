---
# lcars-vp1f
title: 'Attach key: open a terminal attached to a host''s Herdr'
status: completed
type: feature
priority: normal
created_at: 2026-10-18T16:51:07Z
updated_at: 2026-10-18T16:58:23Z
parent: lcars-0jge
---

Key opens an iTerm2 window running herdr locally, ssh -t <alias> herdr, mosh <alias> -- herdr, or ssh -t <alias> tmux attach. Settings: target alias (or local) and mode. Pure command builder is unit-tested; osascript opens the window.

## Summary of Changes

AttachAction with per-key target/mode settings and property inspector; attachCommand builder rejects anything but a plain alias (tested). openWindowScript runs the command via /bin/zsh -lc in a new iTerm window. Live check blocked by the macOS Automation consent dialog that appeared on Vik's screen (he was away); the AppleScript itself was accepted by iTerm up to that point. Placed on XL row 4: LOCAL herdr, Mini herdr, Mini mosh.
