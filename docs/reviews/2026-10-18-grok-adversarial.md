# Adversarial review — LCARS — 2026-10-18

Read-only pass over Ops (`packages/ops`) and the Stream Deck plugin (`packages/plugin`), plus `scripts/install.sh` and the launchd template. `cargo test` in `packages/ops` passed (53). `npm test` in `packages/plugin` passed (73). Those suites do not cover the failures below.

## 1. Resubscribe drops the old stream and can stick new panes forever

Severity: blocking

File: `packages/ops/crates/ops/src/herdr/client.rs:149-162`

What breaks: On `pane_created`, Ops opens a replacement `events.subscribe` and drops the previous `Lines` as soon as the new ack arrives. Anything already buffered on the old socket is discarded. Herdr does not replay it on the new connection. That includes `pane.agent_status_changed`, `pane_closed`, and `pane_agent_detected` for every pane, and the new pane's own first status. `docs/herdr-api.md` records that one bad `pane_id` fails the whole subscribe (`pane_not_found`). If a dropped `pane_closed` leaves a dead id in `pane_ids`, every later resubscribe fails, the new id is popped (`client.rs:159`), and the old stream is kept. The old stream is not subscribed to the new pane, so that pane never receives status. A failed resubscribe is not retried until another pane is created. There is no follow-up `session.snapshot` after the swap, so a lost `blocked` stays lost until that pane changes again. `keyedSessions` (`packages/plugin/src/core/store.ts:54-56`) hides panes whose `agent` is still null, so a lost first status means the agent never appears on the deck.

How to reproduce: Extend `fake_herdr` so the first subscribe writes `pane_created` for `w9:p7` and then `pane.agent_status_changed` `blocked` for `w9:p2`, and the second subscribe acks and stays silent (real Herdr does not replay). Assert the client emits `StatusChanged` blocked for `w9:p2`. Today `new_pane_triggers_a_resubscribe_with_the_added_pane` (`client.rs:561`) replays the same push list on every connection, so it only checks the pane id list. A second test: close `w9:p2` in the dropped buffer, then create `w9:p8`. The subscribe that includes `w9:p2` returns `pane_not_found` and `w9:p8` never appears in a later subscribe.

Suggested fix: Do not drop unread frames. After the new ack, `session.snapshot` and replace that host's sessions (the snapshot path already exists). On `pane_not_found`, rebuild `pane_ids` from the snapshot and subscribe once more. A second connection used only for the new pane avoids tearing down the live stream.

## 2. A finished turn is `idle` after every snapshot

Severity: blocking

File: `packages/ops/crates/ops/src/model.rs:230-244`

What breaks: `derive_state` (`model.rs:165-173`) turns idle plus `completion_seq == state_change_seq` on an unfocused agent into `done`. That runs only from `AgentRefreshed` (`model.rs:345`), which the client requests after an idle *event* (`client.rs:176-178`). `Snapshot` copies `pane.agent_status` and `overlay_agent` never calls `derive_state`. The recorded Claude turn (`docs/herdr-api.md`) is `working` → `idle` on the stream; `done` exists only on `agent.get`. After every connect, reconnect, or the resubscribe fix above, a finished agent paints idle (grey) until it goes idle *again*. `PaneFocused` can clear `done` (`model.rs:371-373`) but nothing in the snapshot sets it. `StatusChanged` then assigns the raw status (`model.rs:267-269`), so a later duplicate idle wipes `done` if the follow-up `agent.get` fails.

How to reproduce: `Fleet::apply` a `Snapshot` whose pane is `agent_status: idle` and whose agent is `finished_agent(false)` from `model.rs:685`. Expect `AgentState::Done`. The current snapshot test (`model.rs:612`) only asserts `Working`.

Suggested fix: In the snapshot agent loop, set `session.state = derive_state(agent)` and `session.focused = agent.focused`. Keep the `StatusChanged` raw write from clobbering `Done` when the incoming status is still `Idle` and `agent.get` has not contradicted it.

## 3. Long press confirms whatever the slot shows at release

Severity: blocking

File: `packages/plugin/src/actions/inbox-key.ts:82-90`

What breaks: `onKeyDown` stores a timestamp. `onKeyUp` calls `sessionFor` again. Slots are indexes into `sessions_sorted` (blocked first, then newest; `model.rs:402-412`). A status change during the 400 ms hold reorders the deck. The key the operator held can now be a different agent, and Conn sends `enter` to that one (`inbox-key.ts:116`). The same lookup sends `focus` to the wrong pane on a short tap. `renderAll` runs on every store update, so the glyph under the finger changes before release. `inbox-key.test.ts` calls `conn()` with an explicit session and never drives `onKeyUp` after a reorder.

How to reproduce: Store two sessions, slot 0 = `mini/w7:p1` blocked. `onKeyDown` on that key. `setView` so `studio/w1:p9` is blocked and sorts first. `onKeyUp` 500 ms later. The `sendKeys` call is `studio/w1:p9`, `["enter"]`.

Suggested fix: Remember `session.id` on key down and act on that id if it is still in the store. If it is gone, alert and send nothing.

## 4. Effort and scroll follow the wrong focused pane

Severity: blocking

File: `packages/plugin/src/plugin.ts:24` and `packages/plugin/src/core/view-state.ts:81-84`

What breaks: Each Herdr host has its own focused pane, and Ops now tracks that via `pane_focused` (`model.rs:362-380`). `focusedSession` is supposed to prefer the deck's host. The plugin passes `os.hostname().split(".")[0].toLowerCase()` (`Mac-Studio` → `mac-studio`). Session hosts are the `ops.toml` names (`studio`, `mini` in `ops.toml.example`). Those strings are different, the `find` misses, and the function returns `focused[0]`. `all` is urgency order, so the dial scrolls and `/effort` prompts the most urgent focused pane in the fleet. `AgentRefreshed` assigns `session.focused = agent.focused` (`model.rs:346`) and does not clear siblings, so one host can have two `focused: true` panes after an idle `agent.get`. `store.test.ts` passes `localHost: "studio"`, which is the config name, not `os.hostname()`.

How to reproduce: Two focused sessions, `mini/w7:p1` blocked and `studio/w1:p1` working. `createFleetStore({ localHost: "mac-studio" }).focusedSession().id` is `mini/w7:p1`. Pressing the effort dial prompts that pane.

Suggested fix: Compare against an explicit deck host in `ops.toml` (or the host whose `socket` is local). When applying `AgentRefreshed` with `focused: true`, clear `focused` on the other sessions of that host, the way `PaneFocused` already does.

## 5. The Plus host filter rewrites the XL wall

Severity: blocking

File: `packages/plugin/src/core/store.ts:66-67` and `packages/plugin/src/core/store.ts:100-102`

What breaks: One plugin process holds one `FleetStore` (`plugin.ts:25`). `sessionAtAbsolute` reads `state.keys`, which is already `filterByHost`. The Plus machine dial and a fleet-bar tap call `setHostFilter` / `cycleHostFilter`. The XL wall uses absolute slots and is specified as the n-th agent across all hosts (`README.md` XL section). After a filter, wall slot 0 is the first agent of that host and every other host disappears. Paging is correctly ignored by the wall (`store.test.ts` only checks that). The filter is not.

How to reproduce: Eleven sessions on `studio` and `mini`, wall slot 0 is the fleet's first id. `setHostFilter("mini")`. `sessionAtAbsolute(0)` changes identity. No test does this.

Suggested fix: Keep the host filter on the paged layout only. `sessionAtAbsolute` should index `all`, or the filter should be per device.

## 6. A dead host still looks live on the keys

Severity: blocking

File: `packages/ops/crates/ops/src/model.rs:220-228` and `packages/plugin/src/actions/inbox-key.ts:155`

What breaks: `Disconnected` sets `connected: false` and leaves every session in place. The key renderer dims only when the Ops WebSocket is down (`live: state.healthy`). A Herdr restart or a dead SSH master keeps full-colour keys at the last state. Conn and focus still target those ids. The strip already treats `host.connected == false` as offline (`packages/plugin/src/core/strip.ts:47-53`). The keys do not. Combined with finding 9, that stale picture can sit there for 30 s.

How to reproduce: Apply `StatusChanged` blocked, then `Disconnected`. The session remains `blocked` and `FleetView.hosts[0].connected` is false. The plugin still paints `live: true` while its own WebSocket is up.

Suggested fix: Pass the owning host's `connected` flag into `renderKeySvg` as `live`. Refuse `focus` / `send_keys` / `prompt` in `actions` when that host is disconnected.

## 7. Anyone on the machine can press Enter for an agent

Severity: blocking

File: `packages/ops/crates/ops/src/server.rs:104-149`

What breaks: `POST /actions` has no token, no `Origin` check, and no `Host` check. `listen` defaults to `127.0.0.1:7677` and `Config::validate` (`config.rs:89-105`) accepts any string, including `0.0.0.0:7677`. Loopback is shared by every account on the Mac. `send_keys` with `["enter"]` is relayed for any session in the fleet; Ops does not require `state == blocked` and has no notion of a long press. The plugin's Conn gate (`inbox-key.ts:87-116`) is only in the key handler. `server.rs` test `send_keys_relays_logical_keys_to_the_agent` is this call with no credential. A browser form POST is stopped by the JSON content type. `curl` from another local user, and a DNS-rebinding page (the `Host` header is whatever the attacker picks), are not. `GET /ws` has the same lack of a check, so pane titles and cwds leave the machine the same way.

How to reproduce: With Ops up and a blocked session `studio/w9:p1`, from another local account: `curl -s -X POST http://127.0.0.1:7677/actions -H 'content-type: application/json' -d '{"action":"send_keys","session":"studio/w9:p1","keys":["enter"]}'`. Herdr receives `agent.send_keys`. No Stream Deck press.

Suggested fix: Refuse a non-loopback `listen` unless the operator sets an explicit flag. Generate a token at startup, mode `0600`, under the state dir; require it on `/actions` and `/ws`. Reject `enter` (and `y`) unless that session is `blocked`.

## 8. A wedged SSH forward never reconnects

Severity: blocking

File: `packages/ops/crates/ops/src/herdr/transport.rs:71-73` and `packages/ops/crates/ops/src/herdr/client.rs:129-137`

What breaks: `connect` returns the first `UnixStream` that accepts. It checks the ControlMaster and rebuilds `-L` only when that connect fails. A master that is half-open, or a forward whose remote end accepts and then stalls, still accepts locally, so Ops never runs `-O cancel` / `-O forward`. The event loop's `next_line()` has no timeout (`REQUEST_TIMEOUT` wraps ping, snapshot, subscribe ack, and action requests only). The host stays `connected` with a frozen fleet. Every idle status also awaits `agent.get` on this same read loop (`client.rs:176-178`) before the next frame is read, so a slow host delays a following `blocked` by up to 10 s even when the socket is healthy. `ssh_connect_without_master_tells_user_how_to_open_one` covers a missing master, which is the path that already works.

How to reproduce: Accept on the forwarded socket and never write a byte. After the snapshot, `session` must surface `Disconnected` within a few seconds. Today `next_line().await` does not return.

Suggested fix: Timeout the event read (the same 10 s, or an idle bound). On timeout, drop the local socket and force the cancel/forward path instead of taking `connect`'s early return. Move `agent.get` / `pane.get` off the read loop so they cannot stall the stream.

## 9. Reconnect backoff never returns to 1 s after a healthy session

Severity: advisory

File: `packages/ops/crates/ops/src/herdr/client.rs:48-74`

What breaks: `backoff` is set back to `BACKOFF_INITIAL` only when `session` returns `Ok(())`. That return is only "the fleet receiver is gone" (`client.rs:78-79`), and `run` then sees `tx.is_closed()` and exits. A session that connected, ran, and then lost the socket returns `Err`. The next sleep uses whatever doubling survived the outage, capped at 30 s. Herdr can be up again for the whole wait. The plugin client does reset on `open` (`packages/plugin/src/ops/client.ts:73`). Nothing in `packages/ops` asserts the daemon's backoff.

How to reproduce: Fail `session` twice (sleeps 1 s then 2 s, backoff becomes 4 s), then return `Err` again after a simulated long success. The next sleep is 4 s and climbs. A counter that resets when `Connected` is sent stays at 1 s.

Suggested fix: Set `backoff = BACKOFF_INITIAL` immediately after a successful ping, before the stream can die.

## 10. The done-ping baseline is the first frame, which is often empty

Severity: advisory

File: `packages/plugin/src/plugin.ts:54-75`

What breaks: The comment says the first view after reconnect only primes the baseline. `previous` is cleared on `!healthy`, then the next healthy view is stored with no pings, and every later view runs `detectFlips`. `detectFlips` treats an unseen `blocked` session as a new flip (`transitions.ts:16-17`). Ops publishes an empty fleet before any host has snapshotted (`main.rs:98-99`), and the WebSocket sends that value on connect (`server.rs:188`). A fast local snapshot can win the race. A Tailscale host usually cannot: its blocked agents arrive in a second frame and each one pings and flashes. `transitions.test.ts` locks this in with `detectFlips([], [idle, blocked])` expecting the blocked id. That is the wrong behaviour for a host's first snapshot and the right behaviour for a pane created later.

How to reproduce: Connect the plugin to a server that sends `{hosts:[{name:"mini",connected:false}],sessions:[]}` and then a view with `mini/w7:p1` blocked. One blocked ping. The operator did nothing.

Suggested fix: When a host's `connected` goes from false to true, copy that host's sessions into `previous` before diffing. Keep the unseen-blocked ping only for a host that was already in the baseline.

## 11. A burst of status events collapses to the last one

Severity: advisory

File: `packages/ops/crates/ops/src/main.rs:131-133` and `packages/ops/crates/ops/src/server.rs:193-201`

What breaks: The fleet is published through `tokio::sync::watch`, which keeps the latest value. The main loop `send`s on every change and then `recv`s. A channel that already holds the next event is ready immediately, so a short burst is applied before the WebSocket task runs. The plugin sees only the last `FleetView`. `working` → `blocked` → `idle` in one burst becomes `working` → `idle`: no red key, a done ping (`transitions.ts:18`), and the blocked dialog was never shown. `websocket_sends_current_view_then_each_change` publishes one change and waits.

How to reproduce: Apply three status events for one pane without awaiting the socket. The WebSocket test client must observe three payloads. Today it can observe one.

Suggested fix: Publish a bounded `mpsc` of views (or a sequence number the plugin can detect as a gap) and have the socket task drain it. A gap should trigger a snapshot for that host.

## 12. A fast dial twist moves one step

Severity: advisory

File: `packages/plugin/src/core/view-state.ts:62` and `packages/plugin/src/core/store.ts:105`

What breaks: The scroll, machine, and effort dials pass `ev.payload.ticks` through. `movePage`, `cycleHost`, `cycleEffort`, and `moveTicker` add `Math.sign(step)`. A rotate event of three detents changes page, host, or effort by one. Terminal scroll uses the full tick count (`dials.ts:52-53`). `store.test.ts` `movePage(3)` lands on the last page either way, because two pages clamp both `1+1` and `1+3` to page 1. `cycleEffort("medium", -3)` expects `"low"` (`view-state.test.ts:60`), which is what `Math.sign` does, so the suite encodes the collapse.

How to reproduce: `cycleHost(["studio","mini","laptop"], null, 3)` is `"studio"`. Three steps from all would be `"laptop"`.

Suggested fix: Add `step` (and wrap), not `Math.sign(step)`. Update the effort test to match a real multi-detent twist.

## 13. Key titles freeze while the agent is working

Severity: advisory

File: `packages/ops/crates/ops/src/herdr/client.rs:246-251`

What breaks: Subscriptions are `pane.created`, `pane.closed`, `pane.agent_detected`, `pane.focused`, and per-pane status. There is no `pane.updated`. Recorded status frames carry no `title` (`docs/herdr-api.md`, Claude section). Title, cwd, and model are filled from the snapshot, from `pane.get` after detection, and from `agent.get` after idle. Claude rewrites the terminal title during a turn. The key keeps the previous title until the turn ends, which is the string the operator uses to tell panes apart.

How to reproduce: Snapshot a pane titled `Coding`. Push `pane_updated` (or a status frame) with a new `terminal_title_stripped` and the same `working` status. The session title stays `Coding`.

Suggested fix: Subscribe to `pane.updated` and merge title, cwd, and focus from it. A status frame that does include `title` is already applied (`model.rs:274-276`); the live frames do not include it.

## 14. Raising the terminal interpolates the app name into AppleScript

Severity: advisory

File: `packages/plugin/src/os/terminal.ts:35`

What breaks: Every focus runs `tell application "${app}" to activate` with `app` from `LCARS_TERMINAL_APP` (`plugin.ts:39`). A double quote in that value breaks out of the string. `osascript` is invoked with `execFile`, so the shell is not involved; AppleScript still runs the injected statements. Attach is in better shape: `attachCommand` (`iterm.ts:18`) rejects anything outside `[A-Za-z0-9._@-]`, and `openWindowScript` escapes `\` and `"` and single-quotes the shell command. The attach tests cover `mini; rm -rf /`.

How to reproduce: `createTerminalActivator({ app: 'iTerm" to do shell script "touch /tmp/lcars-osascript" tell application "iTerm' }).activate()` and observe `/tmp/lcars-osascript`. The dictation script does not take user text.

Suggested fix: Reuse `appleScriptString` for the application name, and reject names that are not a single token.

## 15. A fresh login can refuse the launchd agent

Severity: advisory

File: `scripts/install.sh:44-49` and `packages/ops/launchd/io.gamov.lcars.ops.plist.template:6-29`

What breaks: The plist is written with the shell's umask. `launchctl bootstrap` rejects a plist that is group- or world-writable, which is what umask `002` produces (`664`). The script never `chmod 644`s it. `plutil -lint` still succeeds, and `scripts/tests/test_install.sh` only lints. Separately, `install -m 0755` keeps `com.apple.quarantine` on a binary that was downloaded. Launchd then SIGKILLs `~/.local/bin/ops` before it logs. A local `cargo build` has no quarantine bit; the release artifact named in the README does. The template itself is a valid per-user agent: absolute `ProgramArguments`, `HOME` set, `PATH` including `/usr/bin` so `ssh` resolves, `RunAtLoad`, and the installer never calls `launchctl` (the shell test checks that).

How to reproduce: `umask 002 scripts/install.sh --home "$tmp" --binary ./ops` and `launchctl bootstrap gui/$UID "$tmp/Library/LaunchAgents/io.gamov.lcars.ops.plist"`. Bootstrap fails while `plutil -lint` passes.

Suggested fix: `chmod 644` the plist after `sed`. `xattr -d com.apple.quarantine` the installed binary when that attribute is present.

## Checked and found sound

- Short tap on an Inbox Key calls `focus` only. Conn runs only when the held time is at least 400 ms and the session state is `blocked`. Stop All arms for a second press and sends `esc`, not `enter`.
- Attach commands are built from a fixed template plus an allowlisted target, then quoted into AppleScript. `execFile` is used, not a shell.
- SSH control uses argument vectors, `BatchMode=yes`, and cancel-then-forward when the local socket connect fails and a master exists. The socket path length is rejected before use. `remote_socket` must be absolute.
- Snapshot replacement drops only the host that reconnected. Protocol mismatch emits `Disconnected` and does not mark the host connected. `pane_focused` clears `done` on the pane that gained focus.
- The plugin WebSocket client resets its own backoff on `open`, ignores a malformed payload, and clears the ping baseline when Ops drops. Stable `blocked` does not ping again. Unseen `idle` does not ping.
- Paging clamps, resets to page 0 when the host filter changes, and the wall ignores the page index. A vanished filter host falls back to all hosts.
- The installer does not load the agent, keeps an existing `ops.toml`, and fails clearly when the binary is missing.

## Verification pass

Checked the tree at `1ec5eb0`, which contains `39359b3` ("Fix the adversarial review findings"). `cargo test` in `packages/ops`: 53 passed (lib), 5 passed (bin), 0 doc-tests; 58 passed, 0 failed. `npm test` in `packages/plugin`: 81 passed (17 files), 0 failed.

#1 — fixed
#2 — fixed
#3 — fixed
#4 — fixed
#5 — fixed
#6 — fixed
#7 — fixed
#8 — partially fixed: a failed health ping unlinks the forward (`reset`) so the next connect cancels and re-forwards, and `agent.get` no longer runs on the read loop. The event read still has no timeout; the ping fires every 30s and then waits up to 10s, so a forward that accepts and answers ping leaves the host `connected` with a frozen fleet, and a forward that accepts and then stalls is noticed about 40s after the snapshot.
#9 — fixed
#10 — partially fixed: the baseline is seeded with a host's sessions on the frame where `connected` flips to true, then replaced with that frame's list. Ops publishes `Connected` with no sessions and the snapshot after that, so the host is already counted as up when the blocked agents arrive and they still ping.
#11 — fixed
#12 — fixed
#13 — not fixed: `subscribe` is still `pane.created`, `pane.closed`, `pane.agent_detected`, `pane.focused`, and per-pane status only (`client.rs` 264–276). A title change while `working` still waits for idle `agent.get` or the next snapshot.
#14 — fixed
#15 — fixed
