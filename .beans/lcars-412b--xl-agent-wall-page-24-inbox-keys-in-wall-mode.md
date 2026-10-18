---
# lcars-412b
title: 'XL agent wall page: 24 Inbox Keys in wall mode'
status: completed
type: feature
priority: normal
created_at: 2026-10-18T16:26:58Z
updated_at: 2026-10-18T16:29:29Z
parent: lcars-bv6p
---

Inbox Key gets a layout setting: paged (Plus, 8 per page) or wall (absolute slot over the whole filtered list). XL gets a new LCARS page with slots 0-23 in rows 1-3, row 4 free for global keys.

## Summary of Changes

Inbox Key gained a `layout` setting: paged (default, follows the Plus scroll dial) or wall (absolute slot over the whole filtered list via store.sessionAtAbsolute). Property inspector offers 32 slots and the layout choice. XL profile (3037CC8B) got a new current page "LCARS" (dir 0F560E0C) with 24 wall keys at 0,0..7,2 written through streamdeck-mcp; its MODEL_LAYOUTS lacks Vik's XL id 20GAT9901 (only 20GAT9902), patched in-process. Verified: page manifest holds 24 io.gamov.lcars.inbox actions with slotIndex 0-23 and layout wall; store tests cover absolute slots.
