#!/usr/bin/env bash
# Checks that every version the repo publishes agrees:
#   packages/ops/Cargo.toml        version = "X.Y.Z"
#   packages/plugin/package.json   "version": "X.Y.Z"
#   packages/plugin/io.gamov.lcars.sdPlugin/manifest.json  "Version": "X.Y.Z.0"
#   CHANGELOG.md                   first "## X.Y.Z — <date>" heading
# Optional argument: a tag such as v0.1.1 that must equal X.Y.Z.
# stdout: JSON {"version": ..., "sources": {...}, "ok": bool}; exit 1 on mismatch.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ROOT="${LCARS_ROOT:-$ROOT}"

read_field() {
  local file="$1" pattern="$2" value
  if ! value=$(grep -oE "$pattern" "$file" | head -n 1); then
    echo "check-versions: no version in $file — add one matching the others" >&2
    exit 1
  fi
  printf '%s' "$value" | grep -oE '[0-9]+\.[0-9]+\.[0-9]+'
}

cargo=$(read_field "$ROOT/packages/ops/Cargo.toml" '^version = "[0-9]+\.[0-9]+\.[0-9]+"')
npm=$(read_field "$ROOT/packages/plugin/package.json" '"version": "[0-9]+\.[0-9]+\.[0-9]+"')
manifest=$(read_field "$ROOT/packages/plugin/io.gamov.lcars.sdPlugin/manifest.json" '"Version": "[0-9]+\.[0-9]+\.[0-9]+\.0"')
changelog=$(read_field "$ROOT/CHANGELOG.md" '^## [0-9]+\.[0-9]+\.[0-9]+ ')

ok=true
for v in "$npm" "$manifest" "$changelog"; do
  [ "$v" = "$cargo" ] || ok=false
done
tag="${1:-}"
if [ -n "$tag" ] && [ "${tag#v}" != "$cargo" ]; then ok=false; fi

printf '{"version":"%s","sources":{"cargo":"%s","npm":"%s","manifest":"%s","changelog":"%s","tag":"%s"},"ok":%s}\n' \
  "$cargo" "$cargo" "$npm" "$manifest" "$changelog" "$tag" "$ok"
if [ "$ok" != true ]; then
  echo "check-versions: versions disagree — set Cargo.toml, package.json (and package-lock.json), manifest.json (X.Y.Z.0) and the top CHANGELOG heading to one version" >&2
  exit 1
fi
