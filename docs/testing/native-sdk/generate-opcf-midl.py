#!/usr/bin/env python3
"""Generate an isolated Win32 MIDL candidate from the staged official inputs."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import sys

spec = importlib.util.spec_from_file_location("stage_opcf", Path(__file__).with_name("stage-opcf-mit-inputs.py"))
staging = importlib.util.module_from_spec(spec)
spec.loader.exec_module(staging)


def verify_source(source):
    for name, (_, expected) in staging.FILES.items():
        data = (source / name).read_bytes()
        actual = hashlib.sha1(b"blob " + str(len(data)).encode("ascii") + b"\0" + data).hexdigest()
        if actual != expected:
            raise ValueError(f"staged input mismatch: {name}")


def generate(source, output, midl):
    if output.exists():
        raise ValueError(f"output already exists: {output}")
    verify_source(source)
    output.mkdir(parents=True, exist_ok=False)
    for name in ("opcerror.h", "opcae_er.h", "opcaedef.h"):
        shutil.copyfile(source / name, output / name)
    # OpcEnum imports opccomn.idl; each proxy's dlldata is separate.
    for name in ("opccomn", "opcda", "opc_ae", "opcSec", "OpcEnum"):
        subprocess.run([midl, "/nologo", "/env", "win32", "/robust", "/Oicf",
                        "/I", str(source.resolve()), "/out", str(output.resolve()),
                        "/h", name + ".h", "/iid", name + "_i.c", "/proxy", name + "_p.c",
                        "/dlldata", name + "_dlldata.c", str((source / (name + ".idl")).resolve())],
                       check=True)
        suffixes = (".h", "_i.c", "_p.c", "_dlldata.c") if name in ("opccomn", "opcda") else (".h", "_i.c")
        for suffix in suffixes:
            if not (output / (name + suffix)).is_file():
                raise ValueError(f"MIDL omitted {name + suffix}")
    output_names = [name + suffix for name in ("opccomn", "opcda", "opc_ae", "opcSec", "OpcEnum")
                    for suffix in (".h", "_i.c")]
    output_names.extend(name + suffix for name in ("opccomn", "opcda") for suffix in ("_p.c", "_dlldata.c"))
    output_names.extend(("opcerror.h", "opcae_er.h", "opcaedef.h"))
    receipt = {name: hashlib.sha256((output / name).read_bytes()).hexdigest() for name in output_names}
    (output / "opcf-receipt.json").write_text(json.dumps(receipt, sort_keys=True) + "\n", encoding="utf-8")


def verify_generated(source, output):
    verify_source(source)
    receipt = json.loads((output / "opcf-receipt.json").read_text(encoding="utf-8"))
    names = {name + suffix for name in ("opccomn", "opcda", "opc_ae", "opcSec", "OpcEnum")
             for suffix in (".h", "_i.c")}
    names.update(name + suffix for name in ("opccomn", "opcda") for suffix in ("_p.c", "_dlldata.c"))
    names.update(("opcerror.h", "opcae_er.h", "opcaedef.h"))
    if set(receipt) != names:
        raise ValueError("MIDL receipt inventory mismatch")
    for name in names:
        if hashlib.sha256((output / name).read_bytes()).hexdigest() != receipt[name]:
            raise ValueError(f"generated file mismatch: {name}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--midl", default="midl.exe")
    parser.add_argument("--verify", action="store_true", help="verify staged and generated outputs")
    args = parser.parse_args()
    try:
        if args.verify:
            verify_generated(args.source, args.output)
        else:
            generate(args.source, args.output, args.midl)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"MIDL candidate generation FAILED: {error}", file=sys.stderr)
        return 1
    print("MIDL inputs verified; ABI and inclusion have NOT been verified")
    return 0


if __name__ == "__main__":
    sys.exit(main())
