---
# lcars-i7ha
title: 'Done ping: flash key and bell on blocked/done transitions'
status: completed
type: feature
priority: normal
created_at: 2026-10-18T16:38:37Z
updated_at: 2026-10-18T16:45:36Z
parent: lcars-a35u
---

Diff successive FleetViews; a session entering blocked or done flashes its key (showAlert) and plays a macOS system sound via afplay. Global setting bell on/off in the property inspector. No alert on the first view after connect.

## Summary of Changes

detectFlips over consecutive healthy views: entering blocked, or working → idle/done, blinks the key via showAlert and plays afplay Ping/Glass. First view after (re)connect primes only. Global Bell setting on/off in the property inspector. Tests for flips and bell.
