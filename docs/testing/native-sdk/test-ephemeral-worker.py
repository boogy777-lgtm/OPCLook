#!/usr/bin/env python3
"""Compare unguarded SDK worker teardown with the production guard."""
import subprocess
import sys


def main():
    if len(sys.argv) != 2:
        raise SystemExit("expected path to softing_core_probe")
    executable = sys.argv[1]
    try:
        baseline = subprocess.run(
            [executable, "--lifetime-baseline"], capture_output=True, timeout=15, check=False
        )
    except subprocess.TimeoutExpired as error:
        if b"lifetime baseline self-destructor reached" not in (error.stderr or b""):
            raise SystemExit("baseline timed out before the intended interleaving")
        print("SDK unguarded baseline: self-wait reproduced (watchdog timeout)", flush=True)
    else:
        raise SystemExit(f"baseline did not self-wait: exit={baseline.returncode}")
    fixed = subprocess.run([executable, "--lifetime-fixed"], timeout=15, check=False)
    if fixed.returncode:
        raise SystemExit(f"guarded SDK worker failed: exit={fixed.returncode}")
    print("SDK guarded worker joined and destroyed on caller: PASS", flush=True)


if __name__ == "__main__":
    main()
