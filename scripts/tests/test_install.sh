#!/usr/bin/env bash
# Harness for scripts/install.sh: installs into a temp home with a fake
# binary, checks every artifact, checks idempotency, and proves launchctl is
# never invoked. Aggregate reporter: each check is independent, so -e is off.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INSTALL="$HERE/../install.sh"
failures=0
check() { # check <description> <command...>
  local desc="$1"; shift
  if "$@"; then echo "ok   $desc"; else echo "FAIL $desc" >&2; failures=$((failures + 1)); fi
}

tmp="$(mktemp -d)"
# shellcheck disable=SC2329 # invoked through the EXIT trap below
cleanup() { rm -rf "$tmp"; return 0; }
trap cleanup EXIT

# Fake ops binary: prints a config on --print-default-config, like the real one.
mkdir -p "$tmp/src" "$tmp/shim"
cat > "$tmp/src/ops" <<'EOF'
#!/bin/sh
[ "$1" = "--print-default-config" ] && echo 'listen = "127.0.0.1:7677"'
exit 0
EOF
chmod +x "$tmp/src/ops"
# launchctl shim that fails loudly: the installer must never load anything.
printf '#!/bin/sh\necho "launchctl must not run" >&2\nexit 99\n' > "$tmp/shim/launchctl"
chmod +x "$tmp/shim/launchctl"

home="$tmp/home"
xattr -w com.apple.quarantine "0083;00000000;Safari;" "$tmp/src/ops"
out="$(umask 002; PATH="$tmp/shim:$PATH" bash "$INSTALL" --home "$home" --binary "$tmp/src/ops" 2>&1)"
rc=$?
check "installer exits 0" test "$rc" -eq 0
check "binary installed" test -x "$home/.local/bin/ops"
check "config written" grep -q 'listen = "127.0.0.1:7677"' "$home/.config/ops/ops.toml"
check "plist rendered with the home path" grep -q "$home/.local/bin/ops" "$home/Library/LaunchAgents/io.gamov.lcars.ops.plist"
check "plist has no placeholder left" bash -c "! grep -q __HOME__ '$home/Library/LaunchAgents/io.gamov.lcars.ops.plist'"

# A release layout: the template next to the script, named on the command line.
rel="$tmp/release"; mkdir -p "$rel"
cp "$HERE/../../packages/ops/launchd/io.gamov.lcars.ops.plist.template" "$rel/"
rel_home="$tmp/home-release"
check "explicit --plist-template renders the plist" bash -c "PATH='$tmp/shim:$PATH' bash '$INSTALL' --home '$rel_home' --binary '$tmp/src/ops' --plist-template '$rel/io.gamov.lcars.ops.plist.template' >/dev/null 2>&1 && grep -q '$rel_home/.local/bin/ops' '$rel_home/Library/LaunchAgents/io.gamov.lcars.ops.plist'"
check "missing template fails with a hint" bash -c "PATH='$tmp/shim:$PATH' bash '$INSTALL' --home '$tmp/home-missing' --binary '$tmp/src/ops' --plist-template '$tmp/nope.template' 2>&1 | grep -q -- '--plist-template'"
check "plist is valid" plutil -lint "$home/Library/LaunchAgents/io.gamov.lcars.ops.plist"
check "plist is 0644 regardless of umask" test "$(stat -f %Lp "$home/Library/LaunchAgents/io.gamov.lcars.ops.plist")" = "644"
check "log dir exists" test -d "$home/Library/Logs/lcars"
check "quarantine flag removed from the installed binary" bash -c "! xattr -p com.apple.quarantine '$home/.local/bin/ops' >/dev/null 2>&1"
check "launchctl was not invoked" bash -c "! grep -q 'launchctl must not run' <<< \"$out\""
check "output tells how to start" grep -q "launchctl bootstrap" <<< "$out"

# Second run keeps the edited config.
echo '# edited' >> "$home/.config/ops/ops.toml"
out2="$(bash "$INSTALL" --home "$home" --binary "$tmp/src/ops" 2>&1)"
check "second run exits 0" test $? -eq 0
check "existing config kept" grep -q '# edited' "$home/.config/ops/ops.toml"
check "second run reports kept config" grep -q "kept existing" <<< "$out2"

# --no-plist skips the agent; missing binary is a clear failure.
home2="$tmp/home2"
bash "$INSTALL" --home "$home2" --binary "$tmp/src/ops" --no-plist >/dev/null 2>&1
check "no-plist leaves no plist" test ! -e "$home2/Library/LaunchAgents/io.gamov.lcars.ops.plist"
err="$(bash "$INSTALL" --home "$tmp/home3" --binary "$tmp/missing" 2>&1)"
rc=$?
check "missing binary exits 1" test "$rc" -eq 1
check "missing binary names the build command" grep -q "cargo build --release" <<< "$err"

if [ "$failures" -eq 0 ]; then echo "all install checks passed"; exit 0; fi
echo "$failures install check(s) failed" >&2
exit 1
