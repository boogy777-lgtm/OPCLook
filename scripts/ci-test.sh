#!/usr/bin/env bash
# CI helper: run `cargo test` with the given arguments; on failure report the
# errors as an annotation (scripts/ci-annotate.sh) and fail the step.
# Usage: scripts/ci-test.sh "<annotation title>" <cargo test args...>
set -o pipefail
title="$1"
shift
if cargo test "$@" 2>&1 | tee test-output.txt; then
  exit 0
fi
"$(dirname "$0")/ci-annotate.sh" "$title" test-output.txt
exit 1
