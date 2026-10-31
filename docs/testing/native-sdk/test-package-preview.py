#!/usr/bin/env python3
"""Negative gates for the private-preview packager, without claiming Win32 interop."""
import importlib.util
from pathlib import Path
import struct
import json
import subprocess
from unittest.mock import patch
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("preview", Path(__file__).with_name("package-preview.py"))
preview = importlib.util.module_from_spec(spec)
spec.loader.exec_module(preview)


def meta():
    packages = [{"id": i, "name": n, "version": "1.0", "source": None, "license": lic}
                for i, n, lic in [("cli", "opclook-cli", "MIT"), ("engine", "opclook-engine", "MIT"),
                                  ("backend", "opclook-backend", "MIT"), ("build", "compiler-helper", None),
                                  ("dev", "test-only", "UNKNOWN")]]
    def dep(pkg, kind): return {"pkg": pkg, "dep_kinds": [{"kind": kind, "target": None}]}
    nodes = [{"id": "cli", "features": [], "deps": [dep("engine", None), dep("dev", "dev")]},
             {"id": "engine", "features": [], "deps": [dep("backend", None)]},
             {"id": "backend", "features": ["native-da-preview"], "deps": [dep("build", "build")]},
             {"id": "build", "features": [], "deps": []}, {"id": "dev", "features": [], "deps": []}]
    return {"packages": packages, "resolve": {"nodes": nodes}}


class PreviewPackagingTests(unittest.TestCase):
    def test_real_dependency_kinds_and_unresolved_license_are_preserved(self):
        rows = preview.normal_build_licenses(meta())
        self.assertEqual({row["name"] for row in rows}, {"opclook-cli", "opclook-engine", "opclook-backend", "compiler-helper"})
        self.assertIsNone(next(row for row in rows if row["name"] == "compiler-helper")["license"])

    def test_feature_and_profile_fail_closed(self):
        data = meta()
        data["resolve"]["nodes"][2]["features"] = []
        with self.assertRaisesRegex(ValueError, "features"):
            preview.normal_build_licenses(data)
        data["resolve"]["nodes"][2]["features"] = ["native-da-preview", "native-da-test-hooks"]
        with self.assertRaisesRegex(ValueError, "features"):
            preview.normal_build_licenses(data)
        good = "OPCL_BUILD_PROBES:BOOL=OFF\nOPCL_USE_LEGACY_OPC:BOOL=OFF\nCMAKE_GENERATOR_PLATFORM:INTERNAL=Win32\n"
        preview.verify_profile(good)
        for bad in (good.replace("PROBES:BOOL=OFF", "PROBES:BOOL=ON"), good.replace("OPC:BOOL=OFF", "OPC:BOOL=ON"), good.replace("Win32", "x64")):
            with self.assertRaises(ValueError): preview.verify_profile(bad)

    def test_reviewed_patches_are_exact_and_untracked_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            sdk = root / "sdk"
            sdk.mkdir()
            here = root / "scripts"
            here.mkdir()
            def git(*args):
                return subprocess.run(["git", "-C", str(sdk), *args], check=True, stdout=subprocess.PIPE).stdout
            git("init", "-q")
            git("config", "user.name", "fixture")
            git("config", "user.email", "fixture@example.invalid")
            files = ["development/core/src/SODaC/a.cpp", "development/core/src/SOClt/b.cpp", "development/core/src/include/c.h"]
            original = {}
            for name in files:
                file = sdk / name
                file.parent.mkdir(parents=True, exist_ok=True)
                file.write_bytes((name + " original\n").encode())
                original[name] = preview.blob_hash(file.read_bytes())
            (sdk / "LICENSE").write_text("license")
            git("add", ".")
            git("commit", "-qm", "baseline")
            commit = git("rev-parse", "HEAD").decode().strip()
            for name in files:
                (sdk / name).write_bytes((name + " patched\n").encode())
            (here / "softing-status-alignment.patch").write_bytes(git("diff", "--", files[0]))
            (here / "softing-com-balance.patch").write_bytes(git("diff", "--", files[1], files[2]))
            (here / "license-sdk-tree.json").write_text(json.dumps({"selection_prefixes": ["development/core/src/"]}))
            with patch.object(preview, "HERE", here), patch.object(preview, "SDK_PIN", commit), patch.object(preview, "PATCHED", original):
                preview.verify_sdk(sdk)
                with (sdk / files[0]).open("ab") as stream:
                    stream.write(b"unreviewed\n")
                with self.assertRaises((ValueError, subprocess.CalledProcessError)):
                    preview.verify_sdk(sdk)
                (sdk / files[0]).write_bytes((files[0] + " patched\n").encode())
                (sdk / "development/core/src/untracked.h").write_text("extra")
                with self.assertRaisesRegex(ValueError, "untracked"):
                    preview.verify_sdk(sdk)

    def test_cargo_log_checks_active_flags_not_declared_features(self):
        good = r'''rustc --cfg "feature=\"native-da-preview\"" --check-cfg "cfg(feature, values(\"native-da-preview\", \"native-da-test-hooks\"))"
[backend] cargo:rustc-link-search=native=C:\build\bridge\Release'''
        bridge = r"C:\build\bridge\Release"
        preview.verify_cargo_log(good, bridge)
        for extra in (r''' --cfg "feature=\"native-da-test-hooks\""''',
                      " CARGO_FEATURE_NATIVE_DA_TEST_HOOKS=1"):
            with self.assertRaisesRegex(ValueError, "test hooks"):
                preview.verify_cargo_log(good + extra, bridge)
        with self.assertRaisesRegex(ValueError, "bridge"):
            preview.verify_cargo_log(good, r"C:\other\Release")
        with self.assertRaisesRegex(ValueError, "preview"):
            preview.verify_cargo_log(good.replace(r'''--cfg "feature=\"native-da-preview\""''', ""), bridge)

    def test_link_map_requires_cli_bridge_and_both_symbols(self):
        good = "opclook-cli softing_da_bridge.lib opcl_da_host_create opcl_da_write_scalar"
        preview.verify_link_map(good)
        for required in ("opclook-cli", "softing_da_bridge", "opcl_da_host_create", "opcl_da_write_scalar"):
            with self.assertRaisesRegex(ValueError, "link map"):
                preview.verify_link_map(good.replace(required, ""))

    def test_toolchain_requires_nonempty_versions(self):
        good = "\n".join(f"{name}: version" for name in ("rustc", "cargo", "cmake", "msvc", "midl", "windows-sdk"))
        preview.verify_toolchain(good)
        with self.assertRaisesRegex(ValueError, "midl"):
            preview.verify_toolchain(good.replace("midl: version", "midl:"))

    def test_bridge_symbols_and_pe_architecture(self):
        preview.check_symbols("_opcl_da_host_create _opcl_da_write_scalar _opcl_da_server_status")
        with self.assertRaisesRegex(ValueError, "test-only"):
            preview.check_symbols("_opcl_da_host_create _opcl_da_write_scalar _opcl_da_test_arm_write_exception")
        with self.assertRaisesRegex(ValueError, "test-only"):
            preview.check_symbols("_opcl_da_host_create _opcl_da_write_scalar _opcl_da_test_future_hook")
        with self.assertRaisesRegex(ValueError, "missing"):
            preview.check_symbols("_opcl_da_server_status")
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "cli.exe"
            data = bytearray(128); data[:2] = b"MZ"; struct.pack_into("<I", data, 60, 80)
            data[80:84] = b"PE\0\0"; struct.pack_into("<H", data, 84, 0x14c); p.write_bytes(data)
            preview.verify_pe_x86(p)
            struct.pack_into("<H", data, 84, 0x8664); p.write_bytes(data)
            with self.assertRaisesRegex(ValueError, "x86"):
                preview.verify_pe_x86(p)
            p.write_bytes(b"MZ")
            with self.assertRaisesRegex(ValueError, "PE"):
                preview.verify_pe_x86(p)


if __name__ == "__main__":
    unittest.main()
