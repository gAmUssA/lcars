---
# lcars-16yu
title: Smaller, unstretched key labels
status: completed
type: task
priority: normal
created_at: 2026-10-18T16:19:01Z
updated_at: 2026-10-18T16:20:00Z
parent: lcars-yeii
---

Vik: key text too big. Go from 26px / 8 chars per line with textLength stretching to 19px / 11 chars per line, three lines, natural width.

## Summary of Changes

Labels now 19 px, up to 11 characters per line, three lines, no textLength stretching, word-aware wrapping (splits inside a word only when it is longer than a line). Verified in the Stream Deck app preview.
