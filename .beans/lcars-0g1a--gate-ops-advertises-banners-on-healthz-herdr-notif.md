---
# lcars-0g1a
title: 'Gate: Ops advertises banners on /healthz, herdr-notify defers'
status: completed
type: task
priority: normal
created_at: 2026-10-20T21:08:58Z
updated_at: 2026-10-20T21:15:45Z
parent: lcars-e15d
---

Ops answers /healthz with header X-Ops-Banners: on when [banners] is enabled. herdr-notify's notify.sh checks it and exits, the same contract claude/stop-hook.sh has with herdr-notify. Header, not body, so existing health checks keep reading ok; unauthenticated is fine because it reveals only that banners are on.

## Summary of Changes

Ops sends `X-Ops-Banners: on` on `/healthz` while `[banners]` is enabled (server.rs, ServeOptions). herdr-notify 0.3.0 (gAmUssA/herdr-notify e1fccf1) checks it and exits; a `standalone` marker in its state dir opts a machine out. Tests on both sides.
