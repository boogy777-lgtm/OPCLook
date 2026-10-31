#!/usr/bin/env python3
"""Check a packaged CLI against the already-running local SDK fixture."""
import json
from pathlib import Path
import subprocess
import sys

if len(sys.argv) != 2:
    raise SystemExit("usage: ci-preview-cli.py PATH_TO_PACKAGED_CLI")
exe = Path(sys.argv[1]).resolve(strict=True)
command = [str(exe), "--da", "localhost", "OPCLook.NativeDaFixture.1", "--timeout", "20", "read", "Fixture.Value"]
result = subprocess.run(command, capture_output=True, text=True, timeout=40, check=False)
print(result.stdout, end="")
print(result.stderr, end="", file=sys.stderr)
if result.returncode:
    raise SystemExit(f"packaged CLI returned {result.returncode}")
try:
    data = json.loads(result.stdout)
    assert data["node_id"] == "Fixture.Value"
    assert data["data"]["value"] == {"type": "i64", "value": "42"}
    assert data["data"]["quality"]["kind"] == "good"
    assert data["data"]["reason"] is None
except (ValueError, KeyError, AssertionError, TypeError) as error:
    raise SystemExit(f"unexpected packaged CLI read result: {error}") from error
print("Packaged CLI local DA fixture read: PASS")
