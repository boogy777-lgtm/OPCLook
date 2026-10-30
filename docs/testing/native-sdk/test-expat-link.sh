#!/usr/bin/env bash
# Linux source-link smoke for the pinned SDK's Expat C translation units.
set -euo pipefail
: "${SOFTING_SDK_ROOT:?Set SOFTING_SDK_ROOT to pinned SDK source}"
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
python3 "$script_dir/verify-softing-sdk.py" --expat
probe="$script_dir/core-probe"
core="$SOFTING_SDK_ROOT/development/core/src"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
objects=()
for name in xmlrole xmltok xmlparse; do
  "${CC:-cc}" -std=c99 -DXML_STATIC -include "$core/SOCmn/expat/linuxconfig.h" \
    -I "$core/include" -I "$core/SOCmn/expat" -c "$core/SOCmn/expat/$name.c" \
    -o "$work/$name.o"
  objects+=("$work/$name.o")
done
"${CC:-cc}" -std=c99 -DXML_STATIC -I "$core/include" "$probe/expat-smoke.c" \
  "${objects[@]}" -o "$work/expat-smoke"
"$work/expat-smoke"
"${CXX:-c++}" -std=c++11 -DXML_STATIC -I "$core/include" "$probe/expat-abi.cpp" \
  "${objects[@]}" -o "$work/expat-abi"
"$work/expat-abi"
printf '%s\n' 'SDK Expat C-to-C++ linkage and callback type passed'
