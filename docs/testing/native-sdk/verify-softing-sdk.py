#!/usr/bin/env python3
"""Verify the SDK revision or exact unpacked public header blobs."""
import hashlib
import os
from pathlib import Path
import subprocess
import sys

root = Path(os.environ["SOFTING_SDK_ROOT"])
expected_commit = "5130e763998189c822882434c43e6be2eb0174cf"
if (root / ".git").exists():
    head = subprocess.run(["git", "-C", str(root), "rev-parse", "HEAD"],
                          check=True, capture_output=True, text=True).stdout.strip()
    if head != expected_commit:
        raise SystemExit(f"SDK HEAD mismatch: expected {expected_commit}, got {head}")
    checked = ["development/C++/src", "development/core/include"]
    if "--core-clean" in sys.argv[1:]:
        checked.append("development/core/src")
    subprocess.run(["git", "-C", str(root), "diff", "--quiet", "HEAD", "--", *checked], check=True)
    extra = subprocess.run(["git", "-C", str(root), "ls-files", "--others",
                            "--exclude-standard", "--", *checked], check=True,
                           capture_output=True, text=True).stdout
    if extra.strip():
        raise SystemExit("unexpected untracked SDK source in verified paths")
else:
    if "--core-clean" in sys.argv[1:]:
        raise SystemExit("core build requires an unmodified pinned Git checkout")
    manifest = Path(__file__).with_name("softing-headers.gitsha")
    for line in manifest.read_text().splitlines():
        expected, name = line.split("  ", 1)
        content = (root / name).read_bytes()
        actual = hashlib.sha1(b"blob " + str(len(content)).encode() + b"\0" + content).hexdigest()
        if actual != expected:
            raise SystemExit(f"SDK header mismatch: {name} expected {expected}, got {actual}")
print(f"SDK public headers verified against {expected_commit}")
