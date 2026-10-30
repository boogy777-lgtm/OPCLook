#!/usr/bin/env python3
"""Detect changes in reviewed SDK source tree and build profile, not legal clearance."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys

HERE = Path(__file__).resolve().parent
PROJECT = HERE.parents[2]
MANIFEST = HERE / "license-sdk-tree.json"


def git(root, *args):
    return subprocess.run(["git", "-C", str(root), *args], check=True,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE).stdout


def verify(sdk, project, manifest):
    info = json.loads(manifest.read_text(encoding="utf-8"))
    commit = info["sdk_commit"]
    actual = git(sdk, "rev-parse", "HEAD").decode().strip()
    if actual != commit:
        raise ValueError(f"SDK HEAD mismatch: {actual}, expected {commit}")
    paths = ["LICENSE", *info["selection_prefixes"]]
    output = git(sdk, "ls-tree", "-r", "-z", commit, "--", *paths)
    observed = {}
    for record in output.split(b"\0"):
        if record:
            metadata, name = record.split(b"\t", 1)
            mode, kind, sha = metadata.decode("ascii").split()
            if kind != "blob" or mode == "120000":
                raise ValueError(f"unexpected SDK entry type: {name!r}")
            observed[name.decode("utf-8")] = sha
    expected = info["paths"]
    if observed != expected:
        changed = sorted(k for k in set(observed) | set(expected) if observed.get(k) != expected.get(k))
        raise ValueError(f"SDK tree manifest mismatch: {changed}")
    # Git's clean filters account for normal Windows CRLF checkout conversion.
    # --no-ext-diff prevents external diff drivers from hiding modifications.
    subprocess.run(["git", "-C", str(sdk), "diff", "--no-ext-diff", "--quiet", "HEAD", "--", *paths], check=True)
    extras = git(sdk, "ls-files", "--others", "-z", "--", *paths)
    if extras:
        raise ValueError(f"untracked SDK inputs: {extras.decode('utf-8', errors='replace')!r}")
    for name, expected_hash in info["profile_sha256"].items():
        content = (project / name).read_bytes().replace(b"\r\n", b"\n")
        actual_hash = hashlib.sha256(content).hexdigest()
        if actual_hash != expected_hash:
            raise ValueError(f"build profile changed: {name}; review and update inventory")
    return len(observed)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("sdk", type=Path, help="clean, unpatched pinned SDK Git checkout")
    args = parser.parse_args()
    try:
        count = verify(args.sdk, PROJECT, MANIFEST)
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        print(f"license source inventory verification FAILED: {error}", file=sys.stderr)
        return 1
    print(f"license source inventory verified: {count} SDK tree entries; legal clearance NOT established")
    return 0


if __name__ == "__main__":
    sys.exit(main())
