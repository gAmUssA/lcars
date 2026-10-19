#!/usr/bin/env bash
# Harness for scripts/check-versions.sh: one agreeing tree it must accept,
# one disagreeing tree and one mismatched tag it must reject.
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPT="$HERE/../check-versions.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"; return 0 2>/dev/null || exit 0' EXIT
fail=0

make_tree() { # dir cargo npm manifest changelog
  local d="$1"
  mkdir -p "$d/packages/ops" "$d/packages/plugin/io.gamov.lcars.sdPlugin"
  printf '[workspace.package]\nversion = "%s"\n' "$2" > "$d/packages/ops/Cargo.toml"
  printf '{\n  "name": "lcars",\n  "version": "%s"\n}\n' "$3" > "$d/packages/plugin/package.json"
  printf '{\n  "Name": "LCARS",\n  "Version": "%s"\n}\n' "$4" > "$d/packages/plugin/io.gamov.lcars.sdPlugin/manifest.json"
  printf '# Changelog\n\n## %s — 2026-10-19\n' "$5" > "$d/CHANGELOG.md"
}

make_tree "$TMP/good" 0.1.1 0.1.1 0.1.1.0 0.1.1
if out=$(LCARS_ROOT="$TMP/good" bash "$SCRIPT" v0.1.1 2>/dev/null) && [[ "$out" == *'"ok":true'* ]]; then
  echo "ok   accepts an agreeing tree and matching tag"
else
  echo "FAIL should accept an agreeing tree: $out"; fail=1
fi

if LCARS_ROOT="$TMP/good" bash "$SCRIPT" v0.2.0 >/dev/null 2>&1; then
  echo "FAIL should reject a tag that differs from the tree"; fail=1
else
  echo "ok   rejects a mismatched tag"
fi

make_tree "$TMP/bad" 0.1.1 0.1.0 0.4.0.0 0.1.1
if LCARS_ROOT="$TMP/bad" bash "$SCRIPT" >/dev/null 2>&1; then
  echo "FAIL should reject disagreeing versions"; fail=1
else
  echo "ok   rejects disagreeing versions"
fi

[ "$fail" -eq 0 ]
