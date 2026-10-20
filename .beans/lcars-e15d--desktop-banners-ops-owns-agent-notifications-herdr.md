---
# lcars-e15d
title: 'Desktop banners: Ops owns agent notifications, herdr-notify steps aside'
status: in-progress
type: epic
priority: normal
created_at: 2026-10-20T21:08:50Z
updated_at: 2026-10-20T21:15:45Z
---

Unify gamussa.notify (herdr-notify) with Ops. Both watched pane.agent_status_changed and both played a sound, so every event rang twice. Ops sees every host, so it owns banners; herdr-notify becomes the fallback for machines without Ops.
