# 0002 — Ops owns desktop banners and reveals the terminal

- Status: accepted
- Date: 2026-10-20

## Context

Two things watched `pane.agent_status_changed` and both rang: the herdr-notify plugin (`gamussa.notify`, a Herdr plugin hook that raises an `alerter` banner) and the LCARS done ping (key blink plus `afplay`). Each event sounded twice. herdr-notify also only sees the machine it runs on, so a Mini agent finishing raises a banner on the Mini's screen, where nobody is looking; Ops already sees that event on the Studio.

herdr-notify's click routing was the better one: it finds the local Herdr client attached to the socket and opens `iterm2:reveal?sessionid=…`, which needs no Automation consent and takes ~20 ms. The plugin raised iTerm with osascript, which needs consent and only brings the app forward.

ADR 0001 says Ops "normalises and relays only; it never re-implements Herdr CLI behaviour". Desktop banners and terminal windows are not Herdr behaviour, so this extends Ops without breaking that rule, but it does give Ops two jobs beyond relaying.

## Decision

- Ops raises banners (`[banners] enabled = true` in `ops.toml`, off by default) through `alerter` on the same transitions as the done ping: entering `blocked`, or `working` → `idle`/`done`, with a reconnect backlog folded in silently. One alerter group per session; the blocked question is the message when Ops read one. Code: `banners.rs`.
- A banner click focuses the pane through `ActionRelay`, so remote panes work, then reveals the terminal.
- Ops reveals the iTerm session hosting a local host's Herdr client after every `focus` action (`reveal.rs`, ported from herdr-notify's `focus.py`). The action response carries `revealed`; the plugin raises iTerm with osascript only when it is false (remote hosts, no attached client).
- Ops advertises banner ownership with `X-Ops-Banners: on` on `/healthz`. herdr-notify checks it and stays silent. Ops that is merely up does not silence herdr-notify. The header reveals nothing beyond that flag, so `/healthz` stays the one unauthenticated endpoint.

## Consequences

- One banner per event on the deck host, for every host. Machines without Ops keep herdr-notify as before.
- The deck bell still sounds alongside a banner with sound; picking one owner is `lcars-6jh4`.
- Reveal reads other processes' argv and an allowlist of environment variables (`KERN_PROCARGS2`). Everything outside the allowlist is discarded unread.
- Only sockets listed in `ops.toml` get banners. A named Herdr session (`~/.config/herdr/sessions/<name>/herdr.sock`) needs its own `[[hosts]]` entry, or its agents go silent once herdr-notify defers.
- The process-introspection and `open` calls are macOS-only; the pure parts (ps parsing, procargs decoding, Herdr session resolution, banner text) are unit-tested.
