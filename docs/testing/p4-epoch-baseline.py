#!/usr/bin/env python3
"""Counterfactual P4 regression: disable only the two epoch guards, then restore."""
import os
import re
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "src/app/table.rs"
TESTS = ["p4_stale_write_form_cannot_cross_connection_epoch",
         "p4_stale_write_confirmation_cannot_cross_connection_epoch"]
MARKERS = ["form", "confirmation"]


def without_guards(original):
    data = original
    for name in MARKERS:
        start = f"        // BEGIN P4 {name} epoch guard\n".encode()
        end = f"        // END P4 {name} epoch guard\n".encode()
        if data.count(start) != 1 or data.count(end) != 1:
            raise ValueError(f"guard markers missing or repeated: {name}")
        before, rest = data.split(start, 1)
        removed, after = rest.split(end, 1)
        if b"return;" not in removed or b"epoch" not in removed:
            raise ValueError(f"unexpected guard content: {name}")
        data = before + after
    return data


def run_tests():
    command = ["cargo", "test", "--locked", "-p", "opclook", "--lib", "--target",
               "x86_64-unknown-linux-gnu", "p4_stale_write_", "--", "--nocapture", "--test-threads=1"]
    environment = dict(os.environ, CARGO_TERM_COLOR="never")
    result = subprocess.run(command, cwd=ROOT, env=environment, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, text=True, errors="replace")
    print(result.stdout, flush=True)
    return result


def main():
    original = SOURCE.read_bytes()
    disabled = without_guards(original)
    try:
        SOURCE.write_bytes(disabled)
        result = run_tests()
        if result.returncode == 0 or "running 2 tests" not in result.stdout:
            raise RuntimeError("counterfactual did not execute and fail both target tests")
        if "test result: FAILED" not in result.stdout or "2 failed" not in result.stdout:
            raise RuntimeError("counterfactual test failure summary missing")
        failures = result.stdout.rsplit("failures:", 1)[-1]
        for test in TESTS:
            if not re.search(r"(?m)^\s*app::tests::" + re.escape(test) + r"\s*$", failures):
                raise RuntimeError(f"counterfactual did not fail target assertion: {test}")
        if "assertion failed" not in result.stdout and "assertion `" not in result.stdout:
            raise RuntimeError("counterfactual did not report a failed assertion")
    finally:
        SOURCE.write_bytes(original)
    result = run_tests()
    if result.returncode != 0 or "running 2 tests" not in result.stdout or "2 passed" not in result.stdout:
        raise RuntimeError("restored epoch guards did not pass both target tests")
    for test in TESTS:
        if f"test app::tests::{test} ..." not in result.stdout:
            raise RuntimeError(f"restored target test did not pass: {test}")
    print("P4 counterfactual guards disabled: both tests fail; restored: both pass")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, RuntimeError) as error:
        print(f"P4 baseline verification FAILED: {error}", file=sys.stderr)
        sys.exit(1)
