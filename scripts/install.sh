#!/usr/bin/env bash
# Install Ops for the current user: binary, config (if absent) and a launchd
# agent plist. Never loads the agent; `make start` does that explicitly.
#
# Usage: scripts/install.sh [--home DIR] [--binary PATH] [--no-plist]
#   --home DIR     install under DIR instead of $HOME (used by the tests)
#   --binary PATH  Ops binary to copy (default packages/ops/target/release/ops)
#   --no-plist     skip the launchd plist
# Exit 0 on success, 1 on a missing prerequisite with the command to fix it.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LABEL="io.gamov.lcars.ops"

install_ops() {
  local home="$HOME" binary="$REPO_ROOT/packages/ops/target/release/ops" plist=1
  while [ $# -gt 0 ]; do
    case "$1" in
      --home) home="$2"; shift 2 ;;
      --binary) binary="$2"; shift 2 ;;
      --no-plist) plist=0; shift ;;
      -h|--help) sed -n '2,10p' "${BASH_SOURCE[0]}"; return 0 ;;
      *) echo "install.sh: unknown argument $1" >&2; return 2 ;;
    esac
  done

  if [ ! -x "$binary" ]; then
    echo "install.sh: Ops binary not found at $binary — build it first: (cd $REPO_ROOT/packages/ops && cargo build --release)" >&2
    return 1
  fi

  local bin_dir="$home/.local/bin" cfg_dir="$home/.config/ops" log_dir="$home/Library/Logs/lcars"
  mkdir -p "$bin_dir" "$cfg_dir" "$log_dir"
  install -m 0755 "$binary" "$bin_dir/ops"
  echo "installed $bin_dir/ops"

  if [ -f "$cfg_dir/ops.toml" ]; then
    echo "kept existing $cfg_dir/ops.toml"
  else
    "$bin_dir/ops" --print-default-config > "$cfg_dir/ops.toml"
    echo "wrote $cfg_dir/ops.toml — edit the [[hosts]] entries before starting"
  fi

  if [ "$plist" -eq 1 ]; then
    local agents_dir="$home/Library/LaunchAgents" target
    mkdir -p "$agents_dir"
    target="$agents_dir/$LABEL.plist"
    sed "s|__HOME__|$home|g" "$REPO_ROOT/packages/ops/launchd/$LABEL.plist.template" > "$target"
    plutil -lint "$target" >/dev/null
    echo "wrote $target (not loaded)"
    echo "to start Ops at login:  launchctl bootstrap gui/\$(id -u) $target"
    echo "to stop and unload:     launchctl bootout gui/\$(id -u)/$LABEL"
  fi
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  install_ops "$@"
fi
