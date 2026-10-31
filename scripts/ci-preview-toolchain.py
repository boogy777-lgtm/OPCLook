#!/usr/bin/env python3
"""Record the selected x86 build tools, not a toolchain authentication."""
import os
from pathlib import Path
import subprocess
import sys

if len(sys.argv) != 2:
    raise SystemExit("usage: ci-preview-toolchain.py OUTPUT")
def version(command, must_succeed=True, require_version=False):
    p = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, errors="replace")
    if must_succeed and p.returncode:
        raise SystemExit(f"tool failed: {command[0]} ({p.returncode})")
    lines = [line.strip() for line in p.stdout.splitlines() if line.strip()]
    if not lines:
        raise SystemExit(f"tool produced no version: {command[0]}")
    if require_version:
        lines = [line for line in lines if "version" in line.casefold()]
        if not lines:
            raise SystemExit(f"tool did not report a version: {command[0]}")
    return lines[0]

sdk = os.environ.get("WindowsSDKVersion", "").strip()
if not sdk:
    raise SystemExit("WindowsSDKVersion is missing; run from x86 vcvars")
versions = {
    "rustc": version(["rustc", "-Vv"]),
    "cargo": version(["cargo", "-V"]),
    "cmake": version(["cmake", "--version"]),
    "msvc": version(["cl"], False, True),
    "midl": version(["midl", "/?"], False, True),
    "windows-sdk": sdk,
}
Path(sys.argv[1]).write_text("".join(f"{key}: {value}\n" for key, value in versions.items()), encoding="utf-8")
