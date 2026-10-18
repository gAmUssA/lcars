# LCARS — Claude Code kickoff

Setup on Studio first:

```
mkdir -p ~/src/lcars/docs && cd ~/src/lcars && git init
# drop HANDOFF.md from Drive into docs/HANDOFF.md
claude
```

Then paste this as the first message:

---

Read docs/HANDOFF.md end to end before doing anything else. This is LCARS: a Stream Deck console for my coding agents. Two components: Ops, a Rust daemon built on kcosr's herdr-compat crate, and a thin Node/TypeScript Stream Deck plugin forked from timvdhoorn/stream-deck-herdr-plugin. Herdr is the only agent source; every host runs it. Hosts are Studio (this Mac Studio, the deck host), Mini (Mac Mini), Laptop (MacBook, travel) and an Arch Linux Mini, all on Tailscale MagicDNS.

Step 1. Write CLAUDE.md from the doc's Naming, Key decisions, Language decisions, Operating decisions and third-sweep gap analysis. Rules, not prose, under 60 lines. Must include: Herdr sockets are server-scoped, so Ops holds one connection per host and merges; subscribe to events, never poll; Conn (unblock/approve) is long-press only, short tap only focuses; no cloud relay, no telemetry, no tmux state scraping; ask me before writing to ~/.claude/settings.json or installing a launchd agent; ask rather than guess SSH aliases or hostnames.

Step 2. Milestone 1, recon. On this machine run herdr --help and herdr --version, connect to ~/.config/herdr/herdr.sock, send events.subscribe, trigger a state change in a test pane and capture the raw pane.agent_status_changed frames. Confirm whether pane_id can be omitted to receive all panes. Compare the Herdr version and protocol number against what herdr-compat targets (v0.9.3, protocol 22) and note any drift. Write everything to docs/herdr-api.md with the recorded frames as fixtures. No daemon or plugin code until that file exists.

Step 3. Scaffold packages/ops (Rust, cargo workspace) and packages/plugin (TypeScript, Elgato SDK v2) with empty crates/packages and a top-level README that points to docs/HANDOFF.md.

Commit at the end of each milestone with a message naming the milestone. Stop and summarise before starting milestone 2.
