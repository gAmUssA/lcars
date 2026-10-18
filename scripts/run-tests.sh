#!/usr/bin/env bash
# Runs every scripts/tests/test_*.sh harness and reports an aggregate.
# Aggregate reporter: harnesses are independent, so -e is off.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
failed=0
count=0
for t in "$HERE"/tests/test_*.sh; do
  [ -e "$t" ] || continue
  count=$((count + 1))
  echo "== $(basename "$t")"
  if ! bash "$t"; then failed=$((failed + 1)); fi
done
if [ "$count" -eq 0 ]; then echo "run-tests.sh: no harnesses found under $HERE/tests" >&2; exit 1; fi
echo "== $count harness(es), $failed failed"
[ "$failed" -eq 0 ]
