#!/usr/bin/env bash
# Use the unmodified SDK checkout at the exact commit documented in the audit.
set -euo pipefail
: "${SOFTING_SDK_ROOT:?Set SOFTING_SDK_ROOT to the SDK source checkout}"
probe_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
python3 "$probe_dir/verify-softing-sdk.py"
"${CXX:-c++}" -std=c++11 -fsyntax-only \
  -I "$SOFTING_SDK_ROOT/development/C++/src" \
  -I "$SOFTING_SDK_ROOT/development/core/include" \
  "$probe_dir/softing-public-api-probe.cpp"
