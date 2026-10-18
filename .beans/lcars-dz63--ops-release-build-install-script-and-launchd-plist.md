---
# lcars-dz63
title: Ops release build, install script and launchd plist (not loaded without a yes)
status: completed
type: task
priority: normal
created_at: 2026-10-18T17:33:05Z
updated_at: 2026-10-18T17:35:37Z
parent: lcars-1wnu
---

scripts/install.sh copies target/release/ops to ~/.local/bin, writes ~/.config/ops/ops.toml if absent and ~/Library/LaunchAgents/io.gamov.lcars.ops.plist; loading is a separate explicit step. Harness test runs it against a temp HOME.

## Summary of Changes

scripts/install.sh installs ~/.local/bin/ops, writes ops.toml only if absent, renders packages/ops/launchd/io.gamov.lcars.ops.plist.template into ~/Library/LaunchAgents (plutil-linted) and prints the launchctl bootstrap command without running it. scripts/tests/test_install.sh covers artifacts, idempotency, --no-plist, missing binary and a launchctl shim that fails if invoked; scripts/run-tests.sh discovers harnesses. Makefile: build, test, pack, install, start, stop, restart, status, uninstall. Not run against Vik's real home: install and start are his call.
