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
    elif "--expat" in sys.argv[1:]:
        checked.extend(["development/core/src/SOCmn/expat", "development/core/src/include/expat.h",
                        "development/core/src/include/expat_external.h"])
    subprocess.run(["git", "-C", str(root), "diff", "--quiet", "HEAD", "--", *checked], check=True)
    extra = subprocess.run(["git", "-C", str(root), "ls-files", "--others",
                            "--exclude-standard", "--", *checked], check=True,
                           capture_output=True, text=True).stdout
    if extra.strip():
        raise SystemExit("unexpected untracked SDK source in verified paths")
else:
    if "--core-clean" in sys.argv[1:]:
        raise SystemExit("core build requires an unmodified pinned Git checkout")
    manifests = ["softing-headers.gitsha"]
    if "--expat" in sys.argv[1:]:
        manifests.append("softing-expat.gitsha")
    for manifest_name in manifests:
        manifest = Path(__file__).with_name(manifest_name)
        for line in manifest.read_text().splitlines():
            expected, name = line.split("  ", 1)
            content = (root / name).read_bytes()
            actual = hashlib.sha1(b"blob " + str(len(content)).encode() + b"\0" + content).hexdigest()
            if actual != expected:
                raise SystemExit(f"SDK source mismatch: {name} expected {expected}, got {actual}")
print(f"SDK public headers verified against {expected_commit}")
