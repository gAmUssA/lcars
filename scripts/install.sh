#!/usr/bin/env bash
# Install Ops for the current user: binary, config (if absent) and a launchd
# agent plist. Never loads the agent; `make start` does that explicitly.
#
# Usage: scripts/install.sh [--home DIR] [--binary PATH] [--plist-template PATH] [--no-plist]
#   --home DIR             install under DIR instead of $HOME (used by the tests)
#   --binary PATH          Ops binary to copy (default packages/ops/target/release/ops)
#   --plist-template PATH  launchd template (default: the repo copy, else the one beside this script)
#   --no-plist             skip the launchd plist
# Exit 0 on success, 1 on a missing prerequisite with the command to fix it.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LABEL="io.gamov.lcars.ops"

install_ops() {
  local home="$HOME" binary="$REPO_ROOT/packages/ops/target/release/ops" plist=1
  local template="$REPO_ROOT/packages/ops/launchd/$LABEL.plist.template"
  # A release download puts install.sh and the template side by side.
  [ -f "$template" ] || template="$(dirname "${BASH_SOURCE[0]}")/$LABEL.plist.template"
  while [ $# -gt 0 ]; do
    case "$1" in
      --home) home="$2"; shift 2 ;;
      --binary) binary="$2"; shift 2 ;;
      --plist-template) template="$2"; shift 2 ;;
      --no-plist) plist=0; shift ;;
      -h|--help) sed -n '2,11p' "${BASH_SOURCE[0]}"; return 0 ;;
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
  # A binary downloaded from a release carries the quarantine flag; launchd
  # kills quarantined executables before they log anything.
  if xattr -p com.apple.quarantine "$bin_dir/ops" >/dev/null 2>&1; then
    xattr -d com.apple.quarantine "$bin_dir/ops"
    echo "removed the quarantine flag from $bin_dir/ops"
  fi
  echo "installed $bin_dir/ops"

  if [ -f "$cfg_dir/ops.toml" ]; then
    echo "kept existing $cfg_dir/ops.toml"
  else
    "$bin_dir/ops" --print-default-config > "$cfg_dir/ops.toml"
    echo "wrote $cfg_dir/ops.toml — edit the [[sessions]] entries before starting"
  fi

  if [ "$plist" -eq 1 ]; then
    local agents_dir="$home/Library/LaunchAgents" target
    mkdir -p "$agents_dir"
    target="$agents_dir/$LABEL.plist"
    if [ ! -f "$template" ]; then
      echo "install.sh: launchd template not found at $template — pass --plist-template PATH or --no-plist" >&2
      return 1
    fi
    sed "s|__HOME__|$home|g" "$template" > "$target"
    # launchctl refuses group- or world-writable plists; do not trust the umask.
    chmod 0644 "$target"
    plutil -lint "$target" >/dev/null
    echo "wrote $target (not loaded)"
    echo "to start Ops at login:  launchctl bootstrap gui/\$(id -u) $target"
    echo "to stop and unload:     launchctl bootout gui/\$(id -u)/$LABEL"
  fi
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  install_ops "$@"
fi
