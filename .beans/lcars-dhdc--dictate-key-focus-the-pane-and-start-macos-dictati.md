---
# lcars-dhdc
title: 'Dictate key: focus the pane and start macOS dictation'
status: completed
type: feature
priority: normal
created_at: 2026-10-18T16:51:07Z
updated_at: 2026-10-18T16:58:23Z
parent: lcars-0jge
---

Press raises iTerm2 and clicks Edit > Start Dictation via System Events (osascript). Verified only as far as the menu lookup; the mic trigger is a hardware check.

## Summary of Changes

DictateAction clicks the Start Dictation item in iTerm2's Edit menu through System Events (menu item verified present by a read-only lookup). The click itself was not run: it would open the microphone on Vik's Mac while he is away.
