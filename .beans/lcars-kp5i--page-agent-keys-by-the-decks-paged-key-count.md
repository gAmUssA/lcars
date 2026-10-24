---
# lcars-kp5i
title: Page agent keys by the deck's paged key count
status: in-progress
type: bug
created_at: 2026-10-24T16:24:08Z
updated_at: 2026-10-24T16:24:08Z
---

The Plus LCARS page always paged agents 8 at a time (PAGE_SIZE). Freeing one key for a Rooms (Previous Page) key would hide every 8th agent. Page by the highest paged slot on the deck plus one, like wallSizeOf does for the wall.

- [x] store pages by a settable page size
- [x] inbox key derives the paged size from placed keys
- [x] scroll dial page count uses it
- [x] tests
- [ ] release (ships with the next tagged release; Unreleased holds other pending changes)
