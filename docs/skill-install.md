# Installing the `lcars` Claude Code skill

The skill at `packages/skill/lcars/SKILL.md` teaches a Claude Code session how LCARS is wired (Ops, plugin, logs) and how to re-author Stream Deck pages with streamdeck-mcp without overwriting your own pages.

## From this repo

```sh
make install-skill
```

This symlinks `packages/skill/lcars` to `~/.claude/skills/lcars`, so edits in the repo are live immediately. Remove it with `rm ~/.claude/skills/lcars` (or `make uninstall`, which also removes Ops).

## From a release

Releases ship no separate skill archive; clone the repo at the release tag and run the same target, or copy the folder by hand:

```sh
git clone --branch vX.Y.Z https://github.com/gAmUssA/lcars.git
mkdir -p ~/.claude/skills
cp -R lcars/packages/skill/lcars ~/.claude/skills/lcars
```

A copied skill does not follow repo updates; prefer the symlink on the machine where the repo lives.

## Verify

In any Claude Code session:

```
/lcars
```

The skill should load and start at its Step 1 (locate the install). It also activates on its own when you mention LCARS, Ops, inbox keys, Conn, or laying out keys and dials on a Stream Deck.

## What the skill needs on the machine

- The repo (default `~/projects/lcars`) for `CLAUDE.md`, `docs/HANDOFF.md` and the plugin sources.
- `uv` and a clone of [streamdeck-mcp](https://github.com/verygoodplugins/streamdeck-mcp) for page authoring; the skill explains the model-id patch original XL decks need.
- Ops reachable on `http://127.0.0.1:7677` to read the live session list.
