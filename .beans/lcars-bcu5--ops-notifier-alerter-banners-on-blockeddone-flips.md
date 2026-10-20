---
# lcars-bcu5
title: 'Ops notifier: alerter banners on blocked/done flips across hosts'
status: completed
type: feature
priority: normal
created_at: 2026-10-20T21:08:58Z
updated_at: 2026-10-20T21:15:45Z
parent: lcars-e15d
---

Port the plugin's detectFlips and per-host backlog baseline to Ops. On a flip, spawn alerter (one group per session) with agent label, project and host, the blocked question when Ops read one. A content click relays focus through ActionRelay, so remote panes work. Off by default ([banners] enabled = false).

## Summary of Changes

`banners.rs`: detect_flips and Baseline ported from the plugin (reconnect backlog primes only), banner text (agent label, project, host when multi-host, blocked question), alerter args with dash guard, alerter lookup (config, PATH, Homebrew). Notifier runs from the main loop on every published view; a click relays focus and reveals, falling back to `open -a iTerm`. `[banners]` in ops.toml, off by default. ADR 0002.
