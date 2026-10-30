#!/usr/bin/env python3
"""Apply the reviewed SDK patch to its exact source blob on any platform."""
import hashlib
from pathlib import Path
import subprocess
import sys

SOURCE_PATH = Path("development/core/src/SODaC/SODaCCallback.cpp")
EXPECTED_BLOB = "59205bb9b6e32641c966d334589fe355bafe273f"
PATCH_PATH = Path(__file__).with_name("softing-status-alignment.patch")


def apply_patch(root: Path, patch_path: Path = PATCH_PATH) -> None:
    source = (root / SOURCE_PATH).read_bytes()
    header = b"blob " + str(len(source)).encode("ascii") + b"\0"
    actual = hashlib.sha1(header + source).hexdigest()
    if actual != EXPECTED_BLOB:
        raise ValueError(f"SDK source mismatch: expected {EXPECTED_BLOB}, got {actual}")
    patch = patch_path.read_bytes().replace(b"\r\n", b"\n")
    if b"\r" in patch:
        raise ValueError("patch contains unsupported carriage return")
    for check in (True, False):
        command = ["git", "-c", "core.autocrlf=false", "apply", "--whitespace=nowarn"]
        if check:
            command.append("--check")
        subprocess.run(command + ["-"], input=patch, cwd=root, check=True)


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("Usage: python softing_patch.py SDK_ROOT")
    apply_patch(Path(sys.argv[1]))
