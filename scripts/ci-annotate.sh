#!/usr/bin/env bash
# CI helper: turn a failed build/test log into ONE GitHub error annotation with
# the compiler errors, the failing tests and their panic output, so they can be
# read from the checks API without downloading the full job log.
# Usage: scripts/ci-annotate.sh "<annotation title>" <log file>
title="$1"
log="$2"
clean="${log}.clean"
tr -d '\r' < "$log" > "$clean"
msg=$( {
  grep -E -A6 "^error(\[E[0-9]+\])?:" "$clean" | head -80
  grep -E "^test .* FAILED$" "$clean"
  grep -E -A25 "^---- .* stdout ----$" "$clean"
} | head -300 | sed -e 's/%/%25/g' | awk 'BEGIN{ORS="%0A"} {print}')
echo "::error title=${title}::${msg}"
