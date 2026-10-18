# Herdr socket API — recon findings

Recorded 2026-10-18 on Daystrom (Mac Studio) against the live local Herdr server. Everything below was observed, not read from docs. Raw frames live in `docs/fixtures/` and are the test fixtures for Ops.

## Versions

| Item | Value | Source |
| --- | --- | --- |
| `herdr --version` | `herdr 0.9.4` | CLI |
| `herdr status` client | version 0.9.4, channel stable, protocol 22, endpoint_protocol_generation 1 | CLI |
| `herdr status` server | running, version 0.9.4, private_protocol 22, compatible | CLI |
| Socket | `~/.config/herdr/herdr.sock` (Unix stream). `herdr-client.sock` and `herdr.sock.agent` also exist; not the API | filesystem |
| Schema | `herdr api schema --json` → protocol 22, schema_version 1 | CLI, saved as `docs/fixtures/herdr-api-schema-0.9.4.json` |
| `ping` response | version 0.9.4, protocol 22, capabilities: live_handoff, detached_server_daemon, endpoint_protocol_generation 1, surface_interest, health_check, ssh_agent_registration | raw socket |

### Drift against kcosr `herdr-compat` (herdr-web @ 92717f4, 2026-10-07)

| Aspect | herdr-compat | Live 0.9.4 | Verdict |
| --- | --- | --- | --- |
| Herdr version baseline | v0.9.3 (README: "vendored compatibility baseline"), runtime floor v0.9.0 | 0.9.4 | Patch drift only |
| `PROTOCOL_VERSION` | 22 | 22 | Match |
| `Method` enum (request methods) | 139 renames | 139 methods in live schema | Exact match, no missing or extra methods |
| `PaneAgentStatusChangedEvent` fields | pane_id, workspace_id, agent_status, agent, display_agent, title, state_labels | same | Match |
| `AgentInfo`, `PaneInfo` fields | same set | same set | Match |
| `ServerCapabilities` | live_handoff, detached_server_daemon, endpoint_protocol_generation, surface_interest, health_check, ssh_agent_registration | same six flags | Match |
| `Subscription` variants | 28 incl. `pane.output_changed` string in tests | live schema has no `pane.output_changed` subscription (it is an EventKind only) | Harmless; test-only string |

Conclusion: `herdr-compat` is usable as-is for protocol 22 / 0.9.4. Ops should still assert `protocol == 22` from the `pong` frame per host at connect time and refuse (with a clear error) on mismatch, the way herdr-web-bridge does.

### Drift against the handoff doc

- `herdr --machine <label-or-id> <command>` **does** forward API commands (`agent list`, `pane list`, `agent prompt`) to a saved SSH machine (`herdr --skill`, "To control a saved SSH machine"). The third-sweep note that the CLI stays strictly server-scoped is outdated for the CLI. It remains true that IDs (`w1:p1`) and agent names are per-server, that there is no merged inventory, and that `machine list` is a profile list. `herdr machine list --json` on Daystrom returns `[]`: no machines are saved yet. Whether `events.subscribe` works through `--machine` forwarding is untested (no profile). Ops design is unchanged: one socket per host, merge locally.
- Handoff says Elgato SDK v2. Current `@elgato/streamdeck` on npm is 3.0.2; Vik approved v3 on 2026-10-18 and `packages/plugin` pins 3.0.2.

## Wire format

- Newline-delimited JSON over the Unix socket. One request per line, one response per line.
- Request: `{"id": "<string>", "method": "<dotted.name>", "params": {...}}`. `id` and `method` required; `params` is required by schema even when empty (`{}`).
- Success: `{"id": "...", "result": {"type": "<snake_case_result>", ...}}`.
- Error: `{"id": "...", "error": {"code": "<snake_case>", "message": "..."}}`. After an error on a subscribe request the server closes the connection (EOF observed).
- **One request per connection.** The server closes the socket after each response; a second request on the same connection fails with `Broken pipe` (tested 2026-10-18). A successful `events.subscribe` is the exception: the connection stays open and pushes event frames after the ack.
- Response `id`s from the CLI look like `cli:agent:list`; choose a distinct prefix (`ops:<host>:<n>`) so frames are attributable in logs.

## Methods Ops will use

| Method | Params | Result type | Notes |
| --- | --- | --- | --- |
| `ping` | `{}` | `pong` | version, protocol, capabilities. Run first on every connect. |
| `session.snapshot` | `{}` | `session_snapshot` | Full state: `snapshot.agents[]` (AgentInfo) plus workspaces/tabs/panes. Use once on connect, then events. |
| `pane.list` | `{workspace_id?}` | `pane_list` | `panes[]` (PaneInfo). Includes panes with no agent (`agent_status: "unknown"`, no `agent`). |
| `agent.list` | `{}` | `agent_list` | `agents[]` (AgentInfo). Only panes with a recognised agent. |
| `events.subscribe` | `{subscriptions: [...]}` | `subscription_started` | Long-lived. See below. |
| `agent.focus` | `{target}` | `ok` | `target` = live agent name or pane id. Marks seen. Short tap → this. |
| `pane.focus` | `{pane_id}` | `ok` | For panes without an agent. |
| `agent.send_keys` | `{target, keys[]}` | `ok` | Logical keys: `enter`, `esc`, `ctrl+c`, `y`, … Conn (long-press) → `enter` or `y` depending on dialog. |
| `pane.send_text` | `{pane_id, text}` | `ok` | Literal text, no Enter. |
| `agent.prompt` | `{target, text, wait?}` | `agent_prompted` | Text + Enter as one submission. Rejects with `agent_blocked` if a dialog is open. |
| `agent.read` / `pane.read` | `{target\|pane_id, source, lines?, strip_ansi?, format?}` | `pane_read` | `source` ∈ visible, recent, recent_unwrapped, detection. Recap on touch strip → `recent_unwrapped`, lines 5. |
| `notification.show` | `{title, body?, sound?, position?}` | `notification_show` | In-TUI toast, not macOS. |

Not to be called from Ops: `server.stop`, `server.live_handoff`, `pane.report_agent` / `pane.release_agent` (those are for agent integrations; Ops is read-only on lifecycle).

## Status model

`AgentStatus` enum (events and info): `idle | working | blocked | done | unknown`.

- `blocked` = Herdr recognised an approval or question UI. This is the Conn trigger.
- `done` vs `idle` is decided by the server's seen state; `agent.focus` marks seen, reads do not. Expect `done` → `idle` transitions after a focus.
- `unknown` = pane has an agent Herdr cannot classify, or no agent at all.
- `pane.report_agent` accepts only `idle | working | blocked | unknown` (`PaneAgentState`); `done` is server-derived.
- Useful AgentInfo fields for keys: `agent` (claude, codex, grok, …), `name` (user label, may be null), `agent_status`, `terminal_title_stripped`, `cwd`, `focused`, `tokens.model`, `tokens.summary`, `state_change_seq`, `completion_seq`, `label` (PaneInfo only; Herdr-side display label).

## Subscriptions

`events.subscribe` takes `subscriptions: Subscription[]`, each `{type: "<kind>", ...filters}`.

Two families of pushed frames:

1. **Subscription events** (dotted `event`): `pane.agent_status_changed`, `pane.output_matched`, `pane.scroll_changed`. These are per-pane and **require `pane_id`**.
2. **Broadcast events** (snake_case `event`): `pane_created`, `pane_closed`, `pane_updated`, `pane_focused`, `pane_exited`, `pane_agent_detected`, `workspace_*`, `tab_*`, `layout_updated`. Subscribed by `{type: "pane.created"}` etc. with no filter.

### Can `pane_id` be omitted for all panes? No.

- `{"type":"pane.agent_status_changed"}` → `{"error":{"code":"invalid_request","message":"invalid request: missing field \`pane_id\` at line 1 column 133"}}` then EOF.
- `agent_status` filter is optional; `null` is accepted and means any status.
- A `pane_id` that does not exist → `{"error":{"code":"pane_not_found","message":"pane w999:p999 not found"}}` then EOF. One bad id fails the whole subscribe request.
- **Many `pane_id`s on one connection work.** A single `events.subscribe` with 14 `pane.agent_status_changed` entries plus `pane.created` and `pane.closed` acked `subscription_started` and delivered the status change for the one pane that moved. The connection stayed open after that pane was closed.
- `pane.updated` does **not** fire on an agent status change (observed: no `pane_updated` frame during working → blocked → idle). Broad subscriptions are not a substitute for per-pane status subscriptions.
- `pane_agent_detected` (broadcast) fires on agent attach and on release (`released: true, final_status`), with no filter. It carries `agent` but not `agent_status`.

### Ops subscription strategy (derived)

Per host, one long-lived connection:

1. `ping` → assert protocol 22.
2. `session.snapshot` → seed state.
3. `events.subscribe` with `pane.created`, `pane.closed`, `pane_agent_detected`, plus one `pane.agent_status_changed {pane_id}` per pane currently in the snapshot.
4. On `pane_created`: there is no "add subscription" method, so open a replacement connection with the new full pane set, then close the old one once the new ack arrives. Alternatively run a second connection just for the new pane and let it accumulate; replace-and-swap is simpler to reason about. Decide in milestone 2.
5. On `pane_closed`: drop the pane from state; the subscription for it is harmless.
6. Reconnect with backoff on EOF; re-run steps 1–3.

## Recorded frames (fixtures)

Full timelines with CLI calls interleaved: `docs/fixtures/herdr-status-flow.json` (28 frames) and `docs/fixtures/herdr-subscribe-edge-cases.json` (14 frames). Each entry is `{t: seconds, tag: connection, frame: raw line or CLI record}`. Test pane `w9:p2` / `w9:p3` was split from this session's own pane, driven with `herdr pane report-agent --source lcars-recon --agent lcars-test`, released, and closed.

Key frames verbatim:

```json
{"id":"lcars-ping-1","result":{"type":"pong","version":"0.9.4","protocol":22,"capabilities":{"live_handoff":true,"detached_server_daemon":true,"endpoint_protocol_generation":1,"surface_interest":true,"health_check":true,"ssh_agent_registration":true}}}
```

```json
{"id":"lcars-A-filtered-status","result":{"type":"subscription_started"}}
{"data":{"agent":"lcars-test","agent_status":"working","pane_id":"w9:p2","workspace_id":"w9"},"event":"pane.agent_status_changed"}
{"data":{"agent":"lcars-test","agent_status":"blocked","pane_id":"w9:p2","workspace_id":"w9"},"event":"pane.agent_status_changed"}
{"data":{"agent":"lcars-test","agent_status":"idle","pane_id":"w9:p2","workspace_id":"w9"},"event":"pane.agent_status_changed"}
{"data":{"agent":"lcars-test","agent_status":"unknown","pane_id":"w9:p2","workspace_id":"w9"},"event":"pane.agent_status_changed"}
```

```json
{"data":{"pane":{"agent_status":"unknown","cwd":"/Users/vikgamov/projects/lcars","focused":false,"foreground_cwd":"/Users/vikgamov/projects/lcars","pane_id":"w9:p2","revision":0,"scroll":{"max_offset_from_bottom":0,"offset_from_bottom":0,"viewport_rows":37},"tab_id":"w9:t1","terminal_id":"term_65e1eb7e7612916","workspace_id":"w9"},"type":"pane_created"},"event":"pane_created"}
{"data":{"agent":"lcars-test","pane_id":"w9:p2","type":"pane_agent_detected","workspace_id":"w9"},"event":"pane_agent_detected"}
{"data":{"agent":"lcars-test","final_status":"unknown","pane_id":"w9:p2","released":true,"type":"pane_agent_detected","workspace_id":"w9"},"event":"pane_agent_detected"}
{"data":{"pane_id":"w9:p2","type":"pane_closed","workspace_id":"w9"},"event":"pane_closed"}
```

```json
{"id":"lcars-B-nofilter-status","error":{"code":"invalid_request","message":"invalid request: missing field `pane_id` at line 1 column 133"}}
{"id":"lcars-E-bogus-pane","error":{"code":"pane_not_found","message":"pane w999:p999 not found"}}
```

Fixture caveat: `lcars-test` was a reported agent with no real process behind it, so Herdr's screen detection released it about 0.5 s after each `working`/`blocked` report, producing the extra `unknown` frames and the `released: true` detections. Real agents do not bounce like this. Tests must treat those `unknown` frames as part of the recorded sequence, not as expected behaviour of a live agent.

Live AgentInfo sample (from `herdr agent list`, this session's pane while working):

```json
{"agent":"claude","agent_session":{"agent":"claude","kind":"id","source":"herdr:claude","value":"4827ac7d-2a1c-40c5-ae52-710fe42318c0"},"agent_status":"working","cwd":"/Users/vikgamov/projects/lcars","focused":true,"foreground_cwd":"/Users/vikgamov/projects/lcars","pane_id":"w9:p1","revision":5,"state_change_seq":4672,"tab_id":"w9:t1","terminal_id":"term_65e1ea01c6a2915","terminal_title":"◑ Coding policy plugin from Tessl","terminal_title_stripped":"Coding policy plugin from Tessl","tokens":{"model":"fable-5-1","summary":"[Request interrupted by user]"},"workspace_id":"w9"}
```

## CLI surface (for reference, not for polling)

`herdr agent {list,get,read,send-keys,prompt,rename,focus,wait,attach,start,explain}`, `herdr pane {list,current,get,layout,process-info,neighbor,edges,focus,resize,zoom,read,rename,input,split,swap,move,close,send-text,send-keys,wait-output,run,report-agent,report-agent-session,release-agent,report-metadata}`, `herdr api {snapshot,schema}`, `herdr machine {list,status,reconnect,add,rename,remove,enable,disable}`, `herdr status`. CLI server errors are JSON on stderr with exit 1; syntax errors exit 2. Set `HERDR_ENV=1` inside managed panes; `HERDR_PANE_ID`, `HERDR_WORKSPACE_ID`, `HERDR_TAB_ID` are injected.

## Remote host: DS9 (recorded 2026-10-18)

- Reached via the ssh alias `ds9` (Tailscale 100.124.8.59); `ds9.local` has no route from Daystrom. Keys live in 1Password's agent, so a non-interactive process cannot authenticate on its own. Vik opened a shared master once with `ssh -fNM -o ControlPath=~/.ssh/cm-ds9 -o ControlPersist=8h ds9`; every later `ssh -o ControlPath=~/.ssh/cm-ds9 ds9 …` rides it with no prompt. Ops will use the same pattern (Vik: "1password controls"; no dedicated passphrase-less key).
- DS9 runs `herdr 0.9.0`, server protocol 22, socket `/Users/vikgamov/.config/herdr/herdr.sock`. Daystrom is on 0.9.4. Both speak protocol 22; the 0.9.0 `pong` lacks the `ssh_agent_registration` capability flag (five flags instead of six). Ops must treat capability flags as optional booleans.
- Unix-socket forwarding works: `ssh -o ControlPath=~/.ssh/cm-ds9 -O forward -L <local.sock>:/Users/vikgamov/.config/herdr/herdr.sock ds9`, then plain NDJSON on `<local.sock>`. The local socket path must be short (macOS `sun_path` limit is 104 bytes; a 117-byte path was rejected with `Bad local forwarding specification`). `-O cancel -L …` tears it down.
- Through the forward: `ping` → pong 0.9.0; `agent.list` → one agent (`w7:p1`, claude, idle, "claude-watch local setup"); `pane.list` → `w7:p1`, `w7:p2`; `events.subscribe` with `pane.created`, `pane.closed` and one `pane.agent_status_changed` per pane → `subscription_started`, connection held open.

```json
{"id":"ops:ds9:1","result":{"type":"pong","version":"0.9.0","protocol":22,"capabilities":{"live_handoff":true,"detached_server_daemon":true,"endpoint_protocol_generation":1,"surface_interest":true,"health_check":true}}}
```

## Open items for milestone 2

- Arch Mini deferred (Vik, 2026-10-18). Voyager (`uss-voyager` on the tailnet) runs its own Ops in travel mode; no inbound connection planned. Whether `--machine` forwarding carries `events.subscribe` is still untested and no longer needed.
- Confirm whether `herdr-compat` builds standalone outside the herdr-web workspace (it pulls `ratatui`, `crossterm`, `interprocess`; heavier than Ops needs). Option: depend on it by git path for schema types only, or vendor just `api/schema/*` + `api/client.rs`.
- Observe real `blocked` payloads from Claude Code and Codex approval dialogs (`state_labels`, `display_agent`, `title`) — the fixtures above have none because the test agent was synthetic.
