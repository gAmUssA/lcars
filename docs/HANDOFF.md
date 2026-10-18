# Stream Deck Agent Console — Claude Code Handoff

Oct 18, 2026 · @Viktor Gamov

## Goal and context

**Naming (Star Trek, matching Vik's hosts Studio = Mac Studio, Mini = Mac Mini, Laptop = MacBook):** the project and Stream Deck plugin are **LCARS**; the Rust bridge daemon is **Ops** (`ops` binary, `ops.toml` config, `ops.sock`); the long-press unblock/approve action is **Conn** ("take the conn"). Repo: `lcars/` with `packages/ops` and `packages/plugin`.

Build a Stream Deck plugin (macOS, Node/TypeScript, Elgato SDK v2) that turns Vik's three idle Stream Decks into a control surface for his coding agents, with Herdr as the primary backend and Claude Code hooks as a secondary signal. Ship it as an installable `.streamDeckPlugin` and integrate it into the daily workflow.

**Hardware on hand**

| Device | Keys | Role |
| --- | --- | --- |
| Stream Deck + (knobs + touch strip) | 8 keys, 4 dials | Home: Herdr console |
| Stream Deck XL | 32 keys | Home: agent status wall |
| Stream Deck mid-size (15-key MK.2 or Neo, confirm model) | 15 keys | Travel: remote-only profile |

**Machines**

- Mac Studio and Mac Mini, both reachable over SSH, both running tmux; Mosh server can be enabled. Hostnames and SSH aliases: ask Vik, do not guess.
- The Stream Deck app runs on whichever Mac the deck is plugged into (usually the Studio at home, the laptop on the road). The plugin must work when the agents live on a *different* machine than the deck.

**Agents in use:** Claude Code, Codex CLI, Grok (CLI / build), and Herdr as the multiplexer that owns their terminals.

**Non-goals for v1:** no cloud relay, no mobile app, no custom firmware. Reuse existing plugins where they already do the job.

## Research findings: what exists and what to reuse

Nothing on the market talks to Herdr; everything else is covered. Install the three marked "install" as-is, and read the source of the ones marked "study" before writing new code.

| Project | What it does | Agents | Use |
| --- | --- | --- | --- |
| [AgentDeck](https://marketplace.elgato.com/product/agentdeck-dce3806b-176e-40f2-be7d-e029bec0f464) (Marketplace) | Session keys per agent: running / waiting / current tool; supports Stream Deck + | Claude Code, Codex, OpenCode, OpenClaw | Install on XL; study its session model |
| [Agent Vitals](https://github.com/tapparello/agent-vitals) (GitHub, free) | 5-hour and weekly usage rings, session status, allow/deny keys for permission prompts | Claude Code | Install; study how it hooks permission prompts |
| [Claude & Codex Usage](https://marketplace.elgato.com/product/claude-codex-usage-f337bf84-516c-4e06-8312-304f00b58236) | Quota gauges for both CLIs | Claude Code, Codex | Install on XL |
| [streamdeck-claude-code](https://github.com/linus-amg/streamdeck-claude-code) (linus-amg) | Injects slash commands into a live session: model, reasoning effort, fast mode, skills | Claude Code | Study; map effort to a Plus dial |
| [claude-code-for-stream-deck](https://github.com/msageha/claude-code-for-stream-deck) (msageha) | Session management via Claude Code hooks; holds permission requests open 24 h | Claude Code | Study hooks server pattern |
| [agentsd](https://github.com/paultyng/agentsd) (paultyng) | Hook-driven session tracker; `npm run hooks:install` writes HTTP hooks into `~/.claude/settings.json`; `/debug/sessions` endpoint | Claude Code | Study; cleanest reference for the plugin scaffold |
| [streamdeck-mcp](https://github.com/verygoodplugins/streamdeck-mcp) | MCP server that lets Claude Code set buttons, pages and actions; ships a `streamdeck-designer` skill | Any | Install; use it to let Claude Code author the deck profiles |
| [OpenHarness](https://github.com/autonomous-ai/openharness) (autonomous-ai) | Open-source daemon + app behind Harness Pro; agent detection across machines, WebRTC relay, inbox | Claude Code, Codex, Cursor | Study daemon's agent detection and multi-machine model |

**Herdr** ([herdr.dev](https://herdr.dev/docs/agents/), [compare](https://herdr.dev/compare/)) is a server/client runtime: a daemon owns real PTYs, every UI (TUI, CLI, SSH) is a client. It tracks each pane's agent as *working / blocked / done / idle* and exposes a CLI plus a socket API that scripts and agents can drive (create panes, inject prompts, read status). It runs inside tmux as the outer environment. That API is the primary data source for this plugin; confirm the exact CLI verbs and socket schema from `herdr --help` and the docs before coding.

**Correction after a second sweep (2026-10-18): Herdr Stream Deck plugins already exist.** The earlier claim that nothing talks to Herdr was wrong. Evaluate these before writing any plugin code; the likely outcome is fork-and-extend rather than greenfield.

| Project | What it does | Gap vs our plan |
| --- | --- | --- |
| [dimer47/streamdeck-herdr](https://github.com/dimer47/streamdeck-herdr) (MIT, packaged `.streamDeckPlugin`) | 28 actions: agent status key (`2⎋ 1▶ 3✓`), unblock, next/prev space, worktree, split pane, server status; polls every 3 s via the `herdr` CLI; never-throw helper, per-action-type timers | No per-pane inbox keys, no dials, no multi-host, no travel/local merge |
| [timvdhoorn/stream-deck-herdr-plugin](https://github.com/timvdhoorn/stream-deck-herdr-plugin) (Bun/Node, tested core) | Mirrors `herdr agent list` onto keys; **already supports a remote Herdr box over SSH** by running the local CLI against a remote socket via an SSH host alias; SVG key rendering | Keypad only, 6-key layout by default, one remote host |
| [Pimpmuckl/herdr-streamdeck](https://github.com/Pimpmuckl/herdr-streamdeck) | Described as a Stream Deck + triage panel for Herdr; PRODUCT.md moved, repo needs a direct look | Unknown; check dial support |
| Herdr Control by [so1omon563](https://marketplace.elgato.com/@so1omon563) (Marketplace) | Navigation plugin for Herdr | Closed source; install to compare UX |
| [kcosr/herdr-web](https://github.com/kcosr/herdr-web) | Web client with a **Rust bridge** (`herdr-web-bridge`) that vendors Herdr's socket protocol, schema and client as a `herdr-compat` crate; targets Herdr v0.9.3, protocol 22 | Not a deck plugin, but its `vendor/herdr-compat` crate is the fastest start for our Rust `agent-bridge` |

Herdr's own [Socket API docs](https://herdr.dev/docs/socket-api) confirm: JSON request/response over `~/.config/herdr/herdr.sock`, long-lived event subscriptions, `pane.report_agent` for state, CLI wrappers recommended over raw socket, and a plugin manifest system (`herdr-plugin.toml`, listed on [herdr.dev/plugins](https://herdr.dev/plugins), 1,266 plugins). Also install [herdr-terminal-notifier](https://github.com/dot/herdr-terminal-notifier) for the done-ping rather than building one.

**Revised recommendation:** fork `timvdhoorn/stream-deck-herdr-plugin` (cleanest architecture, remote-host support already there, tested core) and extend it with: per-pane inbox keys, Plus dials, multi-host merge (local + Studio), travel profile. Borrow dimer47's unblock/space/worktree actions. Rust bridge: decided, build it, starting from kcosr's `herdr-compat` crate. The bridge is the one component that merges multiple Herdr hosts (local + Studio) and subscribes to socket events instead of polling the CLI; no existing plugin does that. Rule: the bridge must not re-implement anything Herdr's CLI or the forked plugin already does (status rendering, unblock, workspace switching); it only normalises and merges state and relays actions.

**Third sweep (2026-10-18): gaps found, one assumption broken.**

1. **`herdr machine` is TUI-only.** Per Herdr's [Connecting machines](https://herdr.dev/docs/connecting-machines) docs, the [blog post](https://herdr.dev/blog/connecting-the-machines) and [discussion #515](https://github.com/herdrdev/herdr/discussions/515): the sidebar merges Local plus saved SSH machines, but the CLI and socket API stay **server-scoped**. Selecting a machine in the TUI does not retarget `herdr pane` or `herdr agent` calls, and `--remote` does not compose with subcommands. So the Studio's local socket does **not** give a god's-eye view. **Consequence:** the bridge must hold one socket connection per host itself (local Unix socket, plus SSH-forwarded sockets for Mini, Arch box, and from the MacBook the Studio) and merge them. This is exactly why the bridge exists; the "Herdr merges for us" shortcut in the architecture section is withdrawn.
2. **Events, not polling.** The socket speaks newline-delimited JSON; `events.subscribe` with `pane.agent_status_changed` pushes state changes and is documented as safe for read-only external clients (see [MegaMicro issue #1](https://github.com/jessewaites/MegaMicro/issues/1) for a worked client spec with backoff reconnect). Both existing deck plugins poll the CLI every few seconds; the bridge subscribes instead. Verify the exact event payload and whether `pane_id` filters can be omitted for "all panes".
3. **Agent CLI is single-server too.** Cross-machine agent discovery is on Herdr's roadmap, not shipped; Herdr Cloud is the planned convenience layer. Do not design around it.
4. **Open questions for Vik** (answers below once given): SSH host aliases for each box; whether the Arch Mini runs Herdr at all or only OpenCode/Pi (if no Herdr, that host needs a different source: OpenCode's own API or hooks); exact mid-size deck model; whether the Plus touch strip should show anything or stay blank; whether unblock from a key should require a long-press to avoid accidental approvals.

**Vik's answers (2026-10-18):**

- Herdr runs on every host, including the Arch Mini. Agents are never launched outside Herdr. So the bridge has exactly one source type: a Herdr socket per host.
- Host addressing: Tailscale MagicDNS names in the bridge config (e.g. `mac-mini`, `arch-mini`, `mac-studio` under the tailnet domain). Optionally read `tailscale status --json` to auto-discover peers and mark them online/offline; MagicDNS config is the baseline, discovery is a nice-to-have.
- Unblock/approve key: long-press required. Short tap only focuses the pane.
- Plus touch strip: use it. Candidate layouts for Claude Code to pick from or make switchable with a dial press:
  1. **Fleet bar**: one segment per host, colored by worst agent state on that host (red = blocked, amber = working, green = idle, grey = offline), tap a segment to filter the keys to that host.
  2. **Inbox ticker**: the oldest blocked agent's name, host and the first line of its prompt, scrolling; swipe left/right to cycle through blocked agents, tap to focus.
  3. **Dial labels**: four cells showing what each dial currently controls (e.g. pane select, host filter, volume/`afplay` ping, scroll) with the current value; stays coherent when dials change mode.
  4. **Usage strip**: today's Claude Code / Codex token or cost usage per provider (reuse the Claude & Codex Usage plugin's data source if it exposes one).

**Touch strip decision (MVP):** fleet bar is the default view; pressing dial 4 toggles to the inbox ticker and back. Dial labels and usage strip are post-MVP.

## Ideas borrowed from Harness Pro

[Harness Pro](https://www.autonomous.ai/harness-pro) is a $300-class always-on desk screen that shows every agent on every machine; the free [Harness app](https://www.autonomous.ai/harness-app) and [OpenHarness](https://github.com/autonomous-ai/openharness) daemon are open source. Replicate these behaviours on the decks instead of buying the device:

1. **Inbox keys.** One key per agent. Colour = state: green working, amber waiting on me, blue done, grey idle. Press = focus that pane. This is the whole value of the device.
2. **Needs-you first.** Sort or highlight so the agent that is blocked on a permission or a question is always the brightest key. Agent Vitals already does the allow/deny part for Claude Code; extend it to Herdr's *blocked* state.
3. **Done ping.** When a pane flips to done, flash the key three times and optionally play a sound (`afplay`). Harness calls this the bell.
4. **Tap and speak.** Key that focuses a pane and triggers macOS dictation (or a Whisper CLI) so the next prompt is spoken. On the Plus, the touch strip shows the pane title while dictating.
5. **Every machine, one surface.** Harness shows agents on the laptop, the desktop and a cloud box together. For us: the plugin polls Herdr on both Macs over SSH and merges the sessions into one list; a dial on the Plus switches which machine's panes fill the keys.
6. **Recap glance.** Harness shows a short recap of what each agent just did. Map this to a long-press that renders the pane's last N lines on the Plus touch strip, or sends them to a notification.
7. **Nothing leaves the machine.** Harness seals keys and keystrokes locally. Match that: SSH only, no relay, no telemetry.

Skip the pet animations and the voice-first dispatch as the primary input; the decks are a button surface, not a screen.

## Target layout per device

**Stream Deck + (home, Herdr console)**

| Control | Action |
| --- | --- |
| Keys 1–8 | Inbox keys: one per Herdr pane, colour by state, press = focus, long-press = recap on touch strip |
| Dial 1 | Scroll the pane list when more than 8 panes (touch strip shows the window) |
| Dial 2 | Switch machine: Mac Studio / Mac Mini / All |
| Dial 3 | Claude Code reasoning effort low → high for the focused pane (via slash command injection, see linus-amg) |
| Dial 4 | Scroll the focused pane's terminal (Harness does this); press = send Enter |
| Touch strip | Pane title, state, last line of output; tap segment = jump |

**Stream Deck XL (home, status wall, 32 keys)**

| Row | Contents |
| --- | --- |
| 1 | AgentDeck session keys (Claude Code, Codex) |
| 2 | Agent Vitals: 5 h ring, weekly ring, Allow, Deny, plus Claude & Codex Usage gauges |
| 3 | Herdr workspace shortcuts: one key per project workspace, press = attach |
| 4 | Global: Stop all agents, Pause all, Open Herdr TUI on Studio, Open Herdr TUI on Mini, Dictate, Mute bell, Profile switch |

**Mid-size deck (travel, 15 keys, remote only)**

| Keys | Contents |
| --- | --- |
| 1–6 | Inbox keys for the 6 most recent panes across both Macs (merged list) |
| 7–8 | Allow / Deny for the focused pane's pending prompt |
| 9–10 | Mosh → Studio, Mosh → Mini (opens terminal, runs `mosh <host> -- herdr attach`) |
| 11–12 | tmux attach fallback for each Mac (`ssh -t <host> tmux attach`) |
| 13 | Dictate into focused pane |
| 14 | Stop all agents (confirm with second press) |
| 15 | Refresh / reconnect |

Profiles: ship two Stream Deck profiles, `agents-home` and `agents-travel`; the plugin exposes a `profile` setting so the same actions bind to either. Default poll interval 3 s at home, 10 s on travel, configurable.

## Architecture and plugin skeleton

Three parts: a small **agent-bridge** daemon that normalises state from Herdr and Claude Code hooks, the **Stream Deck plugin** that renders it, and a **Claude Code skill** so the deck can be re-authored by an agent.

**Wiring diagram**

&#91;embedded content: wiring diagram: host, remotes, travel laptop\]

Only the two accented boxes are new code, and the same two binaries run on the Studio and the MacBook. The Mini, the Arch box and the phone need nothing from this project; they just run Herdr (or Moshi) and are reached over Tailscale.

**Data flow**

1. `agent-bridge` (**Rust**, built on kcosr's `herdr-compat` crate; runs on the Mac with the deck) holds one Herdr socket connection per configured host — the local `~/.config/herdr/herdr.sock` plus SSH-forwarded sockets over Tailscale for the other hosts — subscribes to `events.subscribe` / `pane.agent_status_changed` (no polling), merges the per-host streams into one normalised session list tagged by host, and relays key actions (focus, long-press unblock, stop) back to the right host's socket. Optional secondary signal: listen on `localhost:PORT` for Claude Code hook POSTs (`SessionStart`, `PreToolUse`, `Notification`, `Stop`) installed into `~/.claude/settings.json` the way agentsd does it.
2. It merges into one normalised list: `{ id, host, workspace, pane, agent: claude|codex|grok|shell, state: working|blocked|done|idle, title, lastLine, pendingPrompt?, updatedAt }`.
3. The Stream Deck plugin subscribes over a local WebSocket and paints keys; actions call back into the bridge (`focus`, `inject`, `allow`, `deny`, `stop`, `recap`).
4. Bridge executes actions over SSH: `herdr focus <pane>`, `herdr send <pane> "<text>"`, or tmux equivalents as fallback.

**Repo layout (proposed)**

```
agent-deck/
  packages/
    bridge/            # Rust daemon (herdr-compat based), launchd plist, config.toml (hosts via MagicDNS, profile)
    plugin/            # @elgato/streamdeck v2 plugin, TypeScript (fork of timvdhoorn/stream-deck-herdr-plugin)
      src/actions/
        inbox-key.ts     # per-pane key, state colour, press/long-press
        machine-dial.ts  # Plus dial: Studio / Mini / Arch / All
        effort-dial.ts   # Plus dial: reasoning effort
        scroll-dial.ts   # Plus dial: pane scroll
        fleet-strip.ts   # Plus touch strip: fleet bar, dial-4 press toggles inbox ticker
        allow-deny.ts
        stop-all.ts
        attach-remote.ts # mosh / ssh+tmux launchers
        dictate.ts
      manifest.json
    skill/             # ~/.claude/skills/agent-deck: lets Claude Code redesign profiles via streamdeck-mcp
  profiles/
    agents-home.streamDeckProfile
    agents-travel.streamDeckProfile
```

**Key decisions**

- SDK: `@elgato/streamdeck` v2 with `streamdeck` CLI (`npm i -g @elgato/cli`), as agentsd and Agent Vitals do. Node 20+.
- Transport: SSH with ControlMaster multiplexing so polling does not re-handshake; Mosh only for interactive attach keys, not for polling.
- State colours and icons generated at runtime with `canvas` or pre-rendered SVGs; no per-state PNG sets.
- Dictation: v1 triggers macOS dictation shortcut via `osascript`; v2 can swap in a local Whisper CLI.
- Keep Claude Code-only features (effort dial, usage rings) behind a capability flag so Codex/Grok panes do not show dead controls.

**Language decisions (settled 2026-10-18)**

- **Bridge daemon: Rust.** One static musl binary, same build on macOS and Linux, no runtime to install on remote hosts. Crates: `tokio`, `russh` or shell out to system `ssh` with ControlMaster, `tokio-tungstenite` for the local WebSocket, `serde_json`. Swift on Linux was considered and rejected for the bridge: Arch is not a first-class Swift target and static linking is fiddly.
- **Plugin shell: Node/TypeScript.** Not a choice; Elgato's SDK and tooling are JavaScript. Keep it thin: paint keys, forward presses to the bridge.
- **Optional macOS companion: Swift.** Later milestone, not v1. Menu bar app on the host MacBook showing the pane list and native notifications.

**Hosts (three, all in `config.yaml`)**

| Host | OS | Agents expected |
| --- | --- | --- |
| Mac Studio | macOS | Claude Code, Codex, Grok |
| Mac Mini | macOS | Claude Code, Codex, Grok |
| Old Mac Mini | Arch Linux (planned) | OpenCode, Pi |

All three run Herdr; the machine dial on the Plus cycles Studio / Mini / Arch / All.

**Herdr first, tmux as fallback only.** Herdr is the backend for v1 because it detects agents and tracks state natively; the bridge targets its CLI/socket API. tmux is only used by the `attach-remote` travel keys and as a last-resort `send-keys` path if a host has no Herdr. Do not build a tmux state-scraper.

**Operating decisions (settled 2026-10-18)**

- **Host = Mac Studio.** The XL and Plus plug directly into the Studio; the Stream Deck app and the bridge run there, always on. The MacBook is not the host at home.
- **Focus model = Herdr `machine` command.** Preferred path: Herdr on the Studio mounts the Mini and Arch box as remote machines, so one local Herdr session is the god's-eye view and "focus" is just `herdr` switching its active pane locally. The bridge then talks to ONE local Herdr daemon, no per-host SSH polling. Fallback only if remote-machine panes do not expose state: per-host polling over SSH as described in Data flow.
- **Network = Tailscale.** Installed on every machine, handles DNS. Travel keys use Tailscale hostnames; no port forwarding, no relay.
- **Terminal = iTerm2.** Drive it via its Python API or AppleScript for tab focus and dictation. Do not switch terminals for this project.
- **Notifications.** Vik already runs a CLI notifier from his dotfiles ([gAmUssA/dotfiles](https://github.com/gAmUssA/dotfiles)); read `.zshrc` and `zsh_custom` there to find whether it is `terminal-notifier` or `alerter` and reuse that path. Also evaluate [herdr-terminal-notifier](https://github.com/dot/herdr-terminal-notifier), which already maps Herdr blocked/done events to macOS notifications and suppresses them when the terminal is frontmost; the bridge's done-ping should coexist with or wrap it, not duplicate it. Phone is out of scope: Vik uses the Moshi iOS app, which already covers Herdr and tmux sessions and agent notifications on mobile. Do not build push.

**Travel mode (MacBook + mid-size deck)**

The MacBook runs its own Stream Deck app and its own copy of the bridge binary. The bridge has two Herdr sources, both active at once:

| Source | How | What it shows |
| --- | --- | --- |
| Local Herdr on the MacBook | local socket | Panes for local coding sessions (Vik codes locally on the road, not only remote) |
| Remote Herdr on the Studio | SSH over Tailscale to the Studio only; the Studio's Herdr already mounts the Mini and Arch box via `machine` | Every home pane |

The bridge merges both into one list, tagged `host: local` or `host: studio|mini|arch`. On the 15-key deck, the inbox keys show the six most recent panes across local and remote, local panes first when they exist; a key cycles the view Local / Home / All. Actions on local panes hit the local Herdr; actions on home panes ride over Tailscale. Sleep/wake: the bridge reconnects to the Studio with backoff and greys out home keys while disconnected; local keys keep working offline.

v2 option, not v1: the Studio bridge serves its normalised state over a Tailscale-only HTTP/WebSocket endpoint, so the laptop plugin connects directly without a local bridge. Deferred because v1 reuses the same binary with a config flag and adds no new server surface.

**Open questions for Claude Code to resolve on the machine**

- [ ] Exact Herdr CLI verbs and whether a JSON status output or socket API is exposed; read `herdr --help` and the docs.
- [ ] Confirm the mid-size deck model (MK.2 15-key vs Neo) to size the travel profile.
- [ ] SSH host aliases for Studio and Mini.
- [ ] Whether Grok's CLI emits any hook or status that Herdr detects, or if it is shell-only.

## Build plan for Claude Code

Work in milestones; each one ends with something Vik can press. Stop and ask before any step that writes to `~/.claude/settings.json` or installs a launchd agent.

1. **Recon (30 min).** Install AgentDeck, Agent Vitals, Claude & Codex Usage and streamdeck-mcp as-is. Clone agentsd, linus-amg/streamdeck-claude-code and openharness into `~/src/ref/`. Run `herdr --help`, capture its status output on both Macs over SSH, and write the findings into `docs/herdr-api.md`.
2. **Bridge v0.** Rust daemon (herdr-compat crate) that connects to the Herdr socket on each configured host, subscribes to agent status events, merges to the normalised schema, logs to stdout, serves `/sessions` and a WebSocket for the plugin. Unit tests with recorded Herdr socket frames.
3. **Plugin v0.** Scaffold with `streamdeck create`. Implement `inbox-key` only: colour by state, press = focus over SSH. Link with `npm run link`, test on the Plus.
4. **Plus dials.** `machine-dial`, `scroll-dial`, `effort-dial` (inject `/model` and effort slash commands, borrowing linus-amg's approach). Touch strip shows pane title and last line.
5. **Claude Code hooks.** Add hook receiver to the bridge, install hooks the agentsd way, implement `allow-deny` and the done-ping flash plus `afplay` bell.
6. **Remote keys.** `attach-remote` (Mosh and ssh+tmux variants), `stop-all` with confirm, `dictate` via `osascript`. Build the `agents-travel` profile on the mid-size deck.
7. **XL wall.** Assemble `agents-home` profile on the XL combining installed plugins and ours. Use streamdeck-mcp plus the skill to generate the layout from a YAML spec so it is reproducible.
8. **Package.** `streamdeck pack` → `.streamDeckPlugin`; launchd plist for the bridge; `README` with install steps; `make install`. Add the `agent-deck` skill to `~/.claude/skills/` so future sessions can edit profiles.

**Definition of done:** plug any of the three decks into either Mac, pick the matching profile, and within 5 seconds see live Herdr pane states; press a key to focus a pane, allow a Claude Code prompt, or attach remotely on the road.

**Sources opened for this handoff:** [AgentDeck](https://marketplace.elgato.com/product/agentdeck-dce3806b-176e-40f2-be7d-e029bec0f464), [Agent Vitals](https://github.com/tapparello/agent-vitals), [agentsd](https://github.com/paultyng/agentsd), [streamdeck-claude-code](https://github.com/linus-amg/streamdeck-claude-code), [streamdeck-mcp](https://github.com/verygoodplugins/streamdeck-mcp), [Herdr docs](https://herdr.dev/docs/agents/), [Herdr compare](https://herdr.dev/compare/), [Harness Pro](https://www.autonomous.ai/harness-pro), [OpenHarness](https://github.com/autonomous-ai/openharness).
