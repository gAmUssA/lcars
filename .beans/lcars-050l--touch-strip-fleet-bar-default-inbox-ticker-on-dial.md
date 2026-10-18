---
# lcars-050l
title: 'Touch strip: fleet bar default, inbox ticker on dial-4 press'
status: completed
type: feature
priority: normal
created_at: 2026-10-18T16:05:03Z
updated_at: 2026-10-18T16:13:43Z
parent: lcars-yeii
---

Each dial action owns a 200x100 strip segment; render the fleet bar (one segment per host coloured by worst state, tap = host filter) and the inbox ticker (oldest blocked agent, tap = focus, swipe = cycle). Custom Encoder layout with a pixmap.

## Summary of Changes

Custom Encoder layout layouts/strip.json with one pixmap; every dial renders its 200×100 slice of a shared 800×100 SVG scene (core/strip.ts). Fleet bar: host blocks coloured by worst state with counts and offline marking, selected host outlined, spare segments show PAGE and EFFORT; tap filters. Ticker: BLOCKED i of n with agent, host and title, tap focuses; 'Nothing needs you' when quiet. Dims when Ops is down. Verified on the Plus via the Stream Deck app preview (screenshot).
