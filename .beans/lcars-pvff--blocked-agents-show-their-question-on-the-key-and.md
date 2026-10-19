---
# lcars-pvff
title: Blocked agents show their question on the key and ticker
status: completed
type: feature
created_at: 2026-10-19T19:47:08Z
updated_at: 2026-10-19T19:47:08Z
parent: lcars-a35u
---

On a blocked status event Ops runs agent.read (visible, 40 rows) off the event loop and extract_prompt() picks the question above the first option marker; Session.pending_prompt is set only while blocked and cleared when the state moves on or the read raced it. Plugin shows it as the key label and the ticker title. Tests use the recorded Claude question and trust screens.
