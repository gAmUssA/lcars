---
# lcars-ccgx
title: Package the plugin with streamdeck pack
status: completed
type: task
priority: normal
created_at: 2026-10-18T17:33:05Z
updated_at: 2026-10-18T17:35:37Z
parent: lcars-1wnu
---

Produce io.gamov.lcars.streamDeckPlugin from the built sdPlugin folder; output gitignored. Document double-click install.

## Summary of Changes

make pack → packages/plugin/dist/io.gamov.lcars.streamDeckPlugin via streamdeck pack. The marketplace icon had to be PNG (288/576 px, rasterised from the SVG with qlmanage, marked linguist-generated).
