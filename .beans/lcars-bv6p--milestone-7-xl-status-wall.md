---
# lcars-bv6p
title: 'Milestone 7: XL status wall'
status: completed
type: milestone
priority: normal
created_at: 2026-10-18T16:04:34Z
updated_at: 2026-10-19T01:55:41Z
---

agents-home profile on the XL combining installed plugins and LCARS; generated from a YAML spec via streamdeck-mcp so it is reproducible.

## Summary of Changes

LCARS rows of the XL wall shipped: agents, workspaces, global keys. Third-party rows (Agent Vitals usage rings, Claude & Codex Usage gauges) deferred until Vik installs them from GitHub/Marketplace at home; AgentDeck skipped as redundant with Inbox Keys.

2026-10-19: Vik installed AI Usage (com.len.limits) and Claude & Codex Usage (com.singerous.ai-limits) from the Marketplace. Their manifests are Elgato-encrypted; action ids taken from bin/plugin.js: singerous claude-usage/codex-usage, len.limits progress/codex.progress. Written as raw actions at XL 6,2 7,2 6,3 7,3 (page now 32/32). Agent Vitals not installed; AgentDeck skipped.
