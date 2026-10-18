---
# lcars-h0kj
title: 'Conn: long-press on a blocked Inbox Key approves'
status: completed
type: feature
priority: normal
created_at: 2026-10-18T16:38:37Z
updated_at: 2026-10-18T16:45:36Z
parent: lcars-a35u
---

Long press (>=400 ms) on a key whose session is blocked sends enter through Ops send_keys; short tap still only focuses; long press on a non-blocked key is ignored with an alert. Never on short tap (CLAUDE.md).

## Summary of Changes

Long press on a blocked key sends Enter via Ops send_keys; other states blink. CONN_KEYS = [enter]: Conn confirms the highlighted option (the trust dialog defaults to No, exit, so this is documented, not an approve). Unit test covers the state gate; proven live on a real blocked Claude session.
