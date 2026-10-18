---
name: lcars
description: Operate and re-author the LCARS Stream Deck console for Herdr coding agents. Use when the user mentions LCARS, Ops, the Stream Deck agent wall, inbox keys, Conn, deck pages or profiles, or wants keys and dials laid out on a Stream Deck.
---

# LCARS Skill

Process steps in order. Do not skip ahead.

## Step 1 — Locate the install

- Repo: the directory holding this skill's `packages/` (default `~/projects/lcars`). Read `CLAUDE.md` and `docs/HANDOFF.md` there before changing anything.
- Ops: binary `~/.local/bin/ops`, config `~/.config/ops/ops.toml`, launchd label `io.gamov.lcars.ops`, log `~/Library/Logs/lcars/ops.log`. `curl -s http://127.0.0.1:7677/sessions` shows the merged view.
- Plugin: `packages/plugin/io.gamov.lcars.sdPlugin` linked into the Stream Deck app; log in its `logs/` folder. Rebuild with `npm run build` and `npm run restart` in `packages/plugin`.
- Proceed immediately to Step 2.

## Step 2 — Pick the change

- Key behaviour, colours, labels: edit `packages/plugin/src/core/render.ts` or the action in `packages/plugin/src/actions/`, run `npm test`, rebuild, restart.
- Host list or transport: edit `~/.config/ops/ops.toml`, then `make restart` (launchd) or restart the `ops` process. Remote hosts need an SSH ControlMaster the user opens: `ssh -fNM -o ControlPath=~/.ssh/cm-<alias> -o ControlPersist=8h <alias>`.
- Deck layout (which key is where): proceed to Step 3. Finish here otherwise.

## Step 3 — Re-author a deck page

- Pages are written with streamdeck-mcp's `ProfileManager` (`write_page`), which quits and relaunches the Stream Deck app. Warn the user that the decks go dark for about ten seconds.
- Never overwrite the user's own pages: create a new page (`create_new=True`) or write only to a page LCARS created.
- Button fields: `key` or `position "col,row"`, `controller` (`keypad` or `encoder`), `plugin_uuid "io.gamov.lcars"`, `action_uuid` one of `io.gamov.lcars.inbox`, `.dial.scroll`, `.dial.machine`, `.dial.effort`, `.dial.pane`, `.attach`, `.stop-all`, `.dictate`, `.mute-bell`, and `settings` (Inbox Key: `slotIndex`, `display`, `layout` paged or wall; Attach: `target`, `mode`).
- Original Stream Deck XL models report `20GAT9901`, which the tool's layout table lacks: set `MODEL_LAYOUTS["20GAT9901"] = {KEYPAD: (8, 4)}` before writing.
- Verify with a screenshot of the Stream Deck app window (`screencapture -x`, crop to the window bounds from System Events). Finish here.
