#!/usr/bin/env python3
"""Stage immutable OPCF-Members Classic IDLs/headers; does not change the build."""
import argparse
import hashlib
from pathlib import Path
import subprocess
import sys

COMMIT = "efe0d1d1ea86a8a727bf26a501a261765e836766"
# destination name -> (path in official tree, Git blob)
FILES = {
    "opcda.idl": ("Source/DataAccess/ProxyStub/opcda.idl", "d7e14cdc91c2a008bb2d365229880584482db0df"),
    "opccomn.idl": ("Source/Common/ProxyStub/opccomn.idl", "b18e7ca32f1e8acd0be6ab8b48745f48367199e0"),
    "OpcEnum.idl": ("Source/Common/ServerEnumerator/OpcEnum.idl", "851bb2acc857ea4f669c176dde523ad93f2e9a83"),
    "opcSec.idl": ("Source/Security/ProxyStub/opcSec.idl", "5d63c06127d857fb9b7910d4e560afe1e0299c5c"),
    "opc_ae.idl": ("Source/AlarmsAndEvents/ProxyStub/opc_ae.idl", "3b81175435fe200da5862be8cc2340afbabf83ef"),
    "opcerror.h": ("Source/DataAccess/ProxyStub/opcerror.h", "a9c6a89f5d8ef43ecbe43d131264d6bdb55d76ef"),
    "opcae_er.h": ("Source/AlarmsAndEvents/ProxyStub/opcae_er.h", "36260452bf7a80c7b065265f9d292a0c87f08f34"),
    "opcaedef.h": ("Source/AlarmsAndEvents/ProxyStub/opcaedef.h", "ed56b072d742f3f383d2563e537062dd7dadc02f"),
}


def git(repo, *args):
    return subprocess.run(["git", "-C", str(repo), *args], check=True,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE).stdout


def stage(repo, output, commit=COMMIT, files=FILES):
    if output.exists():
        raise ValueError(f"output already exists: {output}")
    actual = git(repo, "rev-parse", "HEAD").decode("ascii").strip()
    if actual != commit:
        raise ValueError(f"OPCF HEAD mismatch: {actual}; expected {commit}")
    content = {}
    for name, (path, expected) in files.items():
        if Path(name).name != name or name in (".", ".."):
            raise ValueError(f"invalid destination: {name}")
        tree_blob = git(repo, "rev-parse", f"{commit}:{path}").decode("ascii").strip()
        if tree_blob != expected:
            raise ValueError(f"tree blob mismatch: {path}")
        data = git(repo, "cat-file", "blob", tree_blob)
        header = b"blob " + str(len(data)).encode("ascii") + b"\0"
        if hashlib.sha1(header + data).hexdigest() != expected:
            raise ValueError(f"content mismatch: {path}")
        if b"OPC Foundation MIT License 1.00" not in data:
            raise ValueError(f"MIT notice missing: {path}")
        content[name] = data
    # No source checkout mutation and no silent overwrite of stale/generated inputs.
    output.mkdir(parents=True, exist_ok=False)
    for name, data in content.items():
        (output / name).write_bytes(data)
    return len(content)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path, help="new directory; must not exist")
    args = parser.parse_args()
    try:
        count = stage(args.repo, args.output)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"OPCF staging FAILED: {error}", file=sys.stderr)
        return 1
    print(f"Staged {count} immutable MIT-labelled inputs at {args.output}; build substitution NOT verified")
    return 0


if __name__ == "__main__":
    sys.exit(main())
