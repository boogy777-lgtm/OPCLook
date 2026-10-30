#!/usr/bin/env python3
"""Exercise the inventory checker with real temporary Git repositories, not a license audit."""
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("license_check", Path(__file__).with_name("verify-license-closure.py"))
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)


def run(root, *args):
    return subprocess.run(["git", "-C", str(root), *args], check=True, capture_output=True).stdout.decode().strip()


class LicenseInventoryTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.sdk = self.root / "sdk"
        self.sdk.mkdir()
        run(self.sdk, "init", "-q")
        run(self.sdk, "config", "user.email", "fixture@example.invalid")
        run(self.sdk, "config", "user.name", "fixture")
        (self.sdk / "LICENSE").write_text("license\n")
        (self.sdk / "src").mkdir()
        (self.sdk / "src" / "one.cpp").write_text("source\n")
        (self.sdk / ".gitignore").write_text("src/ignored.cpp\n")
        run(self.sdk, "add", ".")
        run(self.sdk, "commit", "-qm", "fixture")
        paths = {}
        for name in ("LICENSE", "src/one.cpp"):
            paths[name] = run(self.sdk, "rev-parse", f"HEAD:{name}")
        self.project = self.root / "project"
        self.project.mkdir()
        (self.project / "CMakeLists.txt").write_text("profile\n")
        self.data = {"sdk_commit": run(self.sdk, "rev-parse", "HEAD"),
                     "selection_prefixes": ["src/"], "paths": paths,
                     "profile_sha256": {"CMakeLists.txt": hashlib.sha256(b"profile\n").hexdigest()}}
        self.manifest = self.root / "manifest.json"
        self.save()

    def save(self):
        self.manifest.write_text(json.dumps(self.data))

    def check(self):
        return checker.verify(self.sdk, self.project, self.manifest)

    def test_clean_and_profile_change(self):
        self.assertEqual(self.check(), 2)
        (self.project / "CMakeLists.txt").write_text("new dependency\n")
        with self.assertRaisesRegex(ValueError, "build profile changed"):
            self.check()

    def test_tracked_modification_and_staged_modification(self):
        (self.sdk / "src" / "one.cpp").write_text("modified\n")
        with self.assertRaises(subprocess.CalledProcessError):
            self.check()
        run(self.sdk, "add", "src/one.cpp")
        with self.assertRaises(subprocess.CalledProcessError):
            self.check()

    def test_untracked_even_ignored_file(self):
        (self.sdk / "src" / "ignored.cpp").write_text("extra source\n")
        with self.assertRaisesRegex(ValueError, "untracked SDK inputs"):
            self.check()

    def test_manifest_or_pin_mismatch(self):
        self.data["paths"]["src/one.cpp"] = "0" * 40
        self.save()
        with self.assertRaisesRegex(ValueError, "tree manifest mismatch"):
            self.check()
        self.data["sdk_commit"] = "0" * 40
        self.save()
        with self.assertRaisesRegex(ValueError, "HEAD mismatch"):
            self.check()


if __name__ == "__main__":
    unittest.main()
