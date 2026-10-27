#!/usr/bin/env bash
# CI helper: turn a failed build/test log into ONE GitHub error annotation with
# the compiler errors, the failing tests and their panic output, so they can be
# read from the checks API without downloading the full job log.
# Usage: scripts/ci-annotate.sh "<annotation title>" <log file>
title="$1"
log="$2"
clean="${log}.clean"
# Cargo colours its output in CI: drop CRs and ANSI escapes before matching.
tr -d '\r' < "$log" | sed -e 's/\x1b\[[0-9;]*[A-Za-z]//g' > "$clean"
picked=$( {
  grep -E -A14 "^error(\[E[0-9]+\])?:" "$clean" | head -150
  grep -E "^test .* FAILED$" "$clean"
  grep -E -A25 "^---- .* stdout ----$" "$clean"
} )
# Nothing recognisable (e.g. a linker or build-script failure): show the tail.
if [ -z "$picked" ]; then
  picked=$(tail -n 80 "$clean")
fi
msg=$(printf '%s\n' "$picked" | head -300 | sed -e 's/%/%25/g' | awk 'BEGIN{ORS="%0A"} {print}')
echo "::error title=${title}::${msg}"
