#!/usr/bin/env python3
"""Check an externally captured MSVC /showIncludes log; no build is performed."""
import argparse
from pathlib import Path
import re
import sys


def norm(value):
    return str(value).replace("\\", "/").casefold().rstrip("/")


def check(log, old_opc_dir, generated_dir, required):
    # Paths, not localized 'Note: including file:' text, are used deliberately.
    data = norm(log)
    old = norm(old_opc_dir)
    generated = norm(generated_dir)
    if len(old) < 4 or len(generated) < 4 or old == generated:
        raise ValueError("invalid trace roots")
    if old + "/" in data:
        raise ValueError("trace mentions old SDK OPC directory")
    missing = []
    for name in required:
        needle = generated + "/" + name.casefold()
        if not re.search(re.escape(needle) + r"(?=[\s\"\r\n]|$)", data):
            missing.append(name)
    if missing:
        raise ValueError(f"required generated includes absent: {missing}")
    return len(required)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--log", required=True, type=Path)
    parser.add_argument("--old-opc-dir", required=True, type=Path)
    parser.add_argument("--generated-dir", required=True, type=Path)
    parser.add_argument("--require", action="append", required=True, help="header basename, repeatable")
    args = parser.parse_args()
    try:
        if any(Path(name).name != name for name in args.require):
            raise ValueError("--require must contain only basenames")
        count = check(args.log.read_text(encoding="utf-8", errors="replace"),
                      args.old_opc_dir, args.generated_dir, args.require)
    except (OSError, ValueError) as error:
        print(f"OPCF include trace FAILED: {error}", file=sys.stderr)
        return 1
    print(f"OPCF trace contains {count} required paths and no old OPC directory; log completeness must be verified separately")
    return 0


if __name__ == "__main__":
    sys.exit(main())
