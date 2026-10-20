---
# lcars-lwh7
title: Ops reveals the iTerm session hosting a local Herdr client
status: completed
type: feature
priority: normal
created_at: 2026-10-20T21:08:58Z
updated_at: 2026-10-20T21:15:45Z
parent: lcars-e15d
---

Port herdr-notify's focus.py routing: find local herdr TUI clients attached to the host's socket (ps + KERN_PROCARGS2, env allowlist), reveal their iTerm session via iterm2:reveal?sessionid=. Used by banner clicks and returned from the focus action as revealed; the plugin falls back to osascript activate only when Ops did not reveal. Removes the Automation prompt from focusing taps.

## Summary of Changes

`reveal.rs`: ps + KERN_PROCARGS2 (env allowlist), Herdr session resolution, iTerm session id check, `open iterm2:reveal`. `focus` returns `revealed`; Inbox Key and dial taps raise iTerm via osascript only when false. Verified live: both local clients (named sessions devoxx-be, kafka-agent) resolved to their sockets and iTerm session ids.
