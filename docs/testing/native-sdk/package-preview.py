#!/usr/bin/env python3
"""Assemble an evidence-bearing private x86 CLI preview; not a release approval."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import shutil
import struct
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
SDK_PIN = "5130e763998189c822882434c43e6be2eb0174cf"
OPCF_PIN = "efe0d1d1ea86a8a727bf26a501a261765e836766"
PATCHED = {
    "development/core/src/SODaC/SODaCCallback.cpp": "59205bb9b6e32641c966d334589fe355bafe273f",
    "development/core/src/SOClt/SOCltEntry.cpp": "9e5304ddd9dd7d1131ebdcf5b05c88a0bac6b8ff",
    "development/core/src/include/SOCltEntry.h": "c2470da726a608e6242285b97d9156bc2885cecc",
}
FORBIDDEN_PREFIX = "opcl_da_test_"


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def run(*args, cwd=None, input_data=None):
    return subprocess.run(list(map(str, args)), cwd=cwd, input=input_data, check=True, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE).stdout


def blob_hash(data):
    return hashlib.sha1(b"blob " + str(len(data)).encode("ascii") + b"\0" + data).hexdigest()


def verify_sdk(sdk):
    if run("git", "-C", sdk, "rev-parse", "HEAD").decode().strip() != SDK_PIN:
        raise ValueError("SDK commit mismatch")
    prefixes = json.loads((HERE / "license-sdk-tree.json").read_text())["selection_prefixes"]
    tracked = run("git", "-C", sdk, "diff", "--name-only", "-z", "HEAD", "--", "LICENSE", *prefixes)
    changed = set(tracked.decode().strip("\0").split("\0")) if tracked else set()
    if changed != set(PATCHED):
        raise ValueError(f"SDK changed files do not match reviewed patches: {sorted(changed)}")
    extras = run("git", "-C", sdk, "ls-files", "--others", "-z", "--", "LICENSE", *prefixes)
    if extras:
        raise ValueError("SDK contains untracked or ignored inputs in the reviewed regions")
    # Reverse each reviewed patch on disposable copies and verify every original
    # blob, so unrelated modifications in a patched file cannot pass unnoticed.
    with tempfile.TemporaryDirectory(prefix="opcl-sdk-verify-") as directory:
        temp = Path(directory)
        for name in PATCHED:
            target = temp / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(sdk / name, target)
        for patch in ("softing-com-balance.patch", "softing-status-alignment.patch"):
            patch_bytes = (HERE / patch).read_bytes().replace(b"\r\n", b"\n")
            if b"\r" in patch_bytes:
                raise ValueError("reviewed patch contains unsupported carriage return")
            run("git", "-c", "core.autocrlf=false", "apply", "--reverse", "--whitespace=nowarn", "-", cwd=temp, input_data=patch_bytes)
        for name, expected in PATCHED.items():
            if blob_hash((temp / name).read_bytes()) != expected:
                raise ValueError(f"SDK patched source differs from reviewed patch: {name}")


def normal_build_licenses(metadata):
    packages = {p["id"]: p for p in metadata["packages"]}
    nodes = {n["id"]: n for n in metadata["resolve"]["nodes"]}
    roots = [p for p in packages.values() if p["name"] == "opclook-cli" and p["id"] in nodes]
    if len(roots) != 1:
        raise ValueError("metadata must contain one resolved opclook-cli")
    todo, seen = [roots[0]["id"]], set()
    while todo:
        current = todo.pop()
        if current in seen:
            continue
        seen.add(current)
        node = nodes[current]
        for dep in node["deps"]:
            if any(k["kind"] in (None, "normal", "build") for k in dep["dep_kinds"]):
                todo.append(dep["pkg"])
    backend = [p for p in packages.values() if p["id"] in seen and p["name"] == "opclook-backend"]
    if len(backend) != 1:
        raise ValueError("CLI dependency graph must contain one backend")
    features = nodes[backend[0]["id"]]["features"]
    if "native-da-preview" not in features or "native-da-test-hooks" in features:
        raise ValueError(f"backend features are not a non-test native preview: {features}")
    return sorted(({"name": packages[x]["name"], "version": packages[x]["version"],
                    "id": x, "source": packages[x].get("source"),
                    "license": packages[x].get("license"), "license_file": packages[x].get("license_file")}
                   for x in seen), key=lambda p: (p["name"], p["version"], p["id"]))


def verify_pe_x86(binary):
    data = binary.read_bytes()
    if len(data) < 64 or data[:2] != b"MZ":
        raise ValueError("CLI is not a PE executable")
    offset = struct.unpack_from("<I", data, 60)[0]
    if offset + 6 > len(data) or data[offset:offset + 4] != b"PE\0\0" or struct.unpack_from("<H", data, offset + 4)[0] != 0x14c:
        raise ValueError("CLI is not a Win32 x86 PE executable")


def verify_profile(cache):
    values = {}
    for line in cache.splitlines():
        if not line or line.startswith(("#", "//")) or "=" not in line or ":" not in line:
            continue
        key, value = line.split("=", 1)
        values[key.split(":", 1)[0]] = value.strip()
    if values.get("OPCL_BUILD_PROBES") != "OFF" or values.get("OPCL_USE_LEGACY_OPC") != "OFF":
        raise ValueError("bridge must use official inputs and OPCL_BUILD_PROBES=OFF")
    if values.get("CMAKE_GENERATOR_PLATFORM", "").lower() != "win32":
        raise ValueError("bridge is not configured for Win32")
    return values


def check_symbols(symbols):
    normalized = symbols.lower()
    if FORBIDDEN_PREFIX in normalized:
        raise ValueError("test-only hook symbols exist in preview bridge")
    if any(name not in normalized for name in ("opcl_da_host_create", "opcl_da_write_scalar")):
        raise ValueError("SDK bridge symbols missing from dumpbin output")


def verify_link_map(link_map):
    normalized = link_map.casefold().replace("_", "-")
    if "opclook-cli" not in normalized:
        raise ValueError("CLI link map does not identify opclook-cli")
    for required in ("softing_da_bridge", "opcl_da_host_create", "opcl_da_write_scalar"):
        if required not in link_map.casefold():
            raise ValueError(f"CLI link map missing {required}")


def verify_cargo_log(log, bridge_release):
    # Cargo also emits --check-cfg listing *declared* features. Only --cfg
    # activates a feature; Windows verbose commands may backslash-escape quotes.
    unescaped = log.replace(r'\"', '"').replace(r"\'", "'")
    def active(name):
        pattern = r"(?<![\w-])--cfg(?:\s+|=)[\"']?feature\s*=\s*[\"']" + re.escape(name) + r"[\"']"
        return re.search(pattern, unescaped, re.IGNORECASE) is not None
    env_hook = re.search(r"(?<![\w])CARGO_FEATURE_NATIVE_DA_TEST_HOOKS\s*=\s*[\"']?1(?!\d)", unescaped, re.IGNORECASE)
    normalized = log.replace("\\", "/").casefold()
    lib_dir = str(bridge_release).replace("\\", "/").casefold()
    if (not active("native-da-preview") or active("native-da-test-hooks") or env_hook
            or lib_dir not in normalized or "rustc-link-search" not in normalized):
        raise ValueError("cargo build log must show active preview, selected bridge path, and no test hooks")


def verify_toolchain(log):
    lines = {line.split(":", 1)[0].strip().casefold(): line.split(":", 1)[1].strip()
             for line in log.splitlines() if ":" in line}
    for name in ("rustc", "cargo", "cmake", "msvc", "midl", "windows-sdk"):
        if not lines.get(name):
            raise ValueError(f"missing toolchain version: {name}")


def assemble(args):
    if args.output.exists():
        raise ValueError("output already exists")
    verify_sdk(args.sdk)
    spec = importlib.util.spec_from_file_location("generate_opcf", HERE / "generate-opcf-midl.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    mod.verify_generated(args.staged, args.generated)
    cache = verify_profile((args.bridge / "CMakeCache.txt").read_text(errors="replace"))
    if Path(cache.get("OPCF_MIT_STAGED", "")).resolve() != args.staged.resolve() or Path(cache.get("OPCF_MIT_GENERATED", "")).resolve() != args.generated.resolve():
        raise ValueError("CMake input roots do not match verified OPCF inputs")
    if Path(cache.get("SOFTING_SDK_ROOT", "")).resolve() != args.sdk.resolve():
        raise ValueError("CMake SDK root does not match verified SDK")
    bridge = args.bridge / "Release" / "softing_da_bridge.lib"
    expat = args.bridge / "Release" / "softing_expat.lib"
    if not bridge.is_file() or not expat.is_file():
        raise ValueError("Release bridge libraries missing")
    symbols = run(args.dumpbin, "/symbols", bridge).decode(errors="replace")
    check_symbols(symbols)
    verify_pe_x86(args.cli)
    verify_link_map(args.link_map.read_text(encoding="utf-8", errors="replace"))
    metadata = json.loads(args.metadata.read_text(encoding="utf-8"))
    licenses = normal_build_licenses(metadata)
    bridge_log = args.bridge_log.read_text(encoding="utf-8", errors="replace")
    cargo_log = args.cargo_log.read_text(encoding="utf-8", errors="replace")
    toolchain_log = args.toolchain_log.read_text(encoding="utf-8", errors="replace")
    if "runtime.cpp" not in bridge_log.lower() or "opcl_da_testing" in bridge_log.lower():
        raise ValueError("bridge log must show runtime compilation without OPCL_DA_TESTING")
    trace_spec = importlib.util.spec_from_file_location("opcf_trace", HERE / "check-opcf-include-trace.py")
    trace = importlib.util.module_from_spec(trace_spec)
    trace_spec.loader.exec_module(trace)
    trace.check(bridge_log, args.sdk / "development/core/src/opc", args.generated,
                ["opcda.h", "opccomn.h", "OpcEnum.h", "opc_ae.h", "opcSec.h", "opcerror.h",
                 "opcda_i.c", "opccomn_i.c", "OpcEnum_i.c", "opc_ae_i.c", "opcSec_i.c"])
    verify_cargo_log(cargo_log, (args.bridge / "Release").resolve())
    verify_toolchain(toolchain_log)
    run("git", "-C", ROOT, "diff", "--quiet", "HEAD", "--")
    revision = run("git", "-C", ROOT, "rev-parse", "HEAD").decode().strip()
    args.output.mkdir(parents=True, exist_ok=False)
    copies = {
        "bin/opclook-cli.exe": args.cli,
        "notices/OPCLook-LICENSE.txt": ROOT / "LICENSE",
        "notices/THIRD-PARTY-NOTICES.txt": HERE / "THIRD-PARTY-NOTICES.txt",
        "evidence/Cargo.lock": ROOT / "Cargo.lock",
        "evidence/cargo-metadata.json": args.metadata,
        "evidence/bridge-build.log": args.bridge_log,
        "evidence/cargo-build.log": args.cargo_log,
        "evidence/cli-link.map": args.link_map,
        "evidence/toolchain.log": args.toolchain_log,
        "evidence/CMakeCache.txt": args.bridge / "CMakeCache.txt",
        "evidence/license-sdk-tree.json": HERE / "license-sdk-tree.json",
        "evidence/softing-expat.gitsha": HERE / "softing-expat.gitsha",
        "evidence/softing-status-alignment.patch": HERE / "softing-status-alignment.patch",
        "evidence/softing-com-balance.patch": HERE / "softing-com-balance.patch",
        "evidence/opcf-receipt.json": args.generated / "opcf-receipt.json",
        "evidence/softing_da_bridge.lib": bridge,
        "evidence/softing_expat.lib": expat,
    }
    for name in PATCHED:
        copies["source/softing-patched/" + name] = args.sdk / name
    for name in mod.staging.FILES:
        copies["source/opcf-input/" + name] = args.staged / name
    for name in json.loads((args.generated / "opcf-receipt.json").read_text()):
        copies["source/opcf-generated/" + name] = args.generated / name
    for destination, source in copies.items():
        target = args.output / destination
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
    (args.output / "evidence/bridge-symbols.txt").write_text(symbols, encoding="utf-8")
    (args.output / "evidence/cargo-declared-licenses.json").write_text(json.dumps(licenses, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    (args.output / "README.txt").write_text(
        "Private engineering preview only; not release or legal clearance.\n"
        "The Cargo normal/build license inventory is a declared-metadata superset,\n"
        "not a PE link map or a complete rights and redistribution review.\n"
        "Review notices, toolchains, dependencies and distribution before release.\n", encoding="utf-8")
    record = {"opclook_commit": revision, "softing_commit": SDK_PIN, "opcf_commit": OPCF_PIN,
              "target": "i686-pc-windows-msvc", "profile": "Release; native-da-preview; no test hooks",
              "scope": "private engineering preview; Cargo normal/build declared licenses are a resolved superset, not a PE link map or legal clearance",
              "files_sha256": {str(p.relative_to(args.output)).replace("\\", "/"): sha256(p.read_bytes())
                               for p in sorted(args.output.rglob("*")) if p.is_file()}}
    (args.output / "manifest.json").write_text(json.dumps(record, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for arg in ("sdk", "staged", "generated", "bridge", "cli", "metadata", "bridge-log", "cargo-log", "toolchain-log", "link-map", "output"):
        parser.add_argument("--" + arg, required=True, type=Path)
    parser.add_argument("--dumpbin", default="dumpbin.exe")
    args = parser.parse_args()
    try:
        assemble(args)
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError, json.JSONDecodeError) as error:
        print(f"private preview packaging FAILED: {error}", file=sys.stderr)
        return 1
    print(f"Private engineering preview assembled at {args.output}; release/legal clearance NOT established")
    return 0


if __name__ == "__main__":
    sys.exit(main())
