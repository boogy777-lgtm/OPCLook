#!/usr/bin/env bash
# Run each COM/VARIANT ownership regression in its own process so a native
# crash identifies the responsible test without suppressing the regular gate.
set -o pipefail
list_file="${RUNNER_TEMP:-.}/opclook-windows-com-tests.txt"
if ! cargo test -p opclook-backend --lib --target i686-pc-windows-msvc -- --list > "$list_file"; then
  echo '::error title=Windows COM test isolation::Could not list backend tests'
  exit 1
fi
mapfile -t tests < <(awk '/^(bff::da_variant::tests::|bff::da::ownership_tests::|bff::com::tests::|winutil::tests::).*: test$/ { sub(/: test$/, ""); print }' "$list_file")
if ((${#tests[@]} == 0)); then
  echo '::error title=Windows COM test isolation::No ownership tests were listed'
  exit 1
fi
failures=0
for test in "${tests[@]}"; do
  printf '\nIsolating %s\n' "$test"
  if cargo test -p opclook-backend --lib --target i686-pc-windows-msvc "$test" -- --exact --nocapture; then
    printf 'Isolated PASS: %s\n' "$test"
  else
    printf '::error title=Windows COM test isolation::Isolated test failed or crashed: %s\n' "$test"
    failures=$((failures + 1))
  fi
done
if ((failures != 0)); then
  printf '%s isolated test(s) failed\n' "$failures"
  exit 1
fi
