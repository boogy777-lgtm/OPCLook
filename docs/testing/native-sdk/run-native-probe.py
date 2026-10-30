#!/usr/bin/env python3
"""Run an SDK probe with bounded time and print its unmodified exit status."""
import argparse
import math
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--timeout", type=float, default=45.0)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command
    if command and command[0] == "--":
        command = command[1:]
    if not command or not math.isfinite(args.timeout) or args.timeout <= 0:
        parser.error("a command and a finite positive timeout are required")
    print(f"native probe starting (timeout={args.timeout}s): {command!r}", flush=True)
    try:
        result = subprocess.run(command, timeout=args.timeout, check=False)
    except subprocess.TimeoutExpired:
        print(f"native probe timed out after {args.timeout}s", flush=True)
        return 1
    except OSError as error:
        print(f"native probe could not start: {error}", flush=True)
        return 1
    print(f"native exit={result.returncode} hex=0x{result.returncode & 0xFFFFFFFF:08X}", flush=True)
    return 0 if result.returncode == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
