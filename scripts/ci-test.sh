#!/usr/bin/env bash
# CI helper: run `cargo test` with the given arguments. On failure, emit the
# compiler errors, the failing tests and their panic output as ONE GitHub error
# annotation, so they can be read from the checks API without the job log.
# Usage: scripts/ci-test.sh "<annotation title>" <cargo test args...>
set -o pipefail
title="$1"
shift
if cargo test "$@" 2>&1 | tee test-output.txt; then
  exit 0
fi
tr -d '\r' < test-output.txt > test-output-clean.txt
msg=$( {
  grep -E -A6 "^error(\[E[0-9]+\])?:" test-output-clean.txt | head -80
  grep -E "^test .* FAILED$" test-output-clean.txt
  grep -E -A25 "^---- .* stdout ----$" test-output-clean.txt
} | head -300 | sed -e 's/%/%25/g' | awk 'BEGIN{ORS="%0A"} {print}')
echo "::error title=${title}::${msg}"
exit 1
