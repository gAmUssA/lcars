---
# lcars-6i3z
title: Derive done from completion_seq
status: todo
type: bug
priority: low
created_at: 2026-10-18T16:45:36Z
updated_at: 2026-10-18T16:45:36Z
parent: lcars-a35u
---

Events never say done; after a turn agent.get has completion_seq == state_change_seq. Ops could derive done for unfocused panes so keys turn green until focused. Needs a pane.get on working→idle or hooks.
