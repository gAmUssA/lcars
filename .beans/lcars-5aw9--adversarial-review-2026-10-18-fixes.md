---
# lcars-5aw9
title: 'Adversarial review 2026-10-18: fixes'
status: completed
type: task
created_at: 2026-10-18T19:21:25Z
updated_at: 2026-10-18T19:21:25Z
parent: lcars-bv6p
---

Grok review docs/reviews/2026-10-18-grok-adversarial.md. Resolution per finding:

- [x] 1 resubscribe: snapshot after every swap; rebuild pane set on failure
- [x] 2 snapshot derives done; repeated idle keeps done
- [x] 3 press acts on the session captured at key down
- [x] 4 Ops marks the local host; plugin prefers it; agent refresh clears sibling focus
- [x] 5 wall indexes the unfiltered list
- [x] 6 keys dim when the host is disconnected; Ops refuses actions with 503
- [x] 7 bearer token (0600 file), loopback-only listen, confirming keys only on blocked
- [x] 8 30 s health ping on a separate connection; transport reset on failure; follow-up reads off the loop
- [x] 9 backoff resets after a successful ping
- [x] 10 ping baseline seeded per host when it (re)connects
- [x] 11 broadcast channel delivers every view in a burst
- [x] 12 dials move by the full tick count
- [ ] 13 subscribe to pane.updated for live titles: deferred, event volume needs a throttle (see lcars follow-up)
- [x] 14 terminal app name validated
- [x] 15 plist chmod 644, quarantine flag stripped
