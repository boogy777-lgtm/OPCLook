#!/usr/bin/env python3
"""Exercise real patch application with LF and Windows CRLF patch files."""
import os
from pathlib import Path
import tempfile
import subprocess
import unittest
from softing_patch import apply_patch, apply_com_patch, COM_BLOBS, PATCH_PATH, SOURCE_PATH


class PatchTests(unittest.TestCase):
    def setUp(self):
        root = Path(os.environ["SOFTING_SDK_ROOT"])
        if (root / ".git").exists():
            self.source = subprocess.run(
                ["git", "-C", str(root), "cat-file", "blob",
                 "59205bb9b6e32641c966d334589fe355bafe273f"],
                check=True, capture_output=True).stdout
        else:
            self.source = (root / SOURCE_PATH).read_bytes()
        self.patch = PATCH_PATH.read_bytes().replace(b"\r\n", b"\n")

    def apply_variant(self, newline):
        with tempfile.TemporaryDirectory(prefix="sdk patch ") as name:
            root = Path(name)
            target = root / SOURCE_PATH
            target.parent.mkdir(parents=True)
            target.write_bytes(self.source)
            patch_file = root / "input patch.diff"
            patch_file.write_bytes(self.patch.replace(b"\n", newline))
            apply_patch(root, patch_file)
            return target.read_bytes()

    def test_lf_and_crlf_apply_identically(self):
        lf = self.apply_variant(b"\n")
        self.assertNotEqual(lf, self.source)
        self.assertEqual(lf, self.apply_variant(b"\r\n"))

    def test_invalid_source_and_patch_do_not_modify_file(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            target = root / SOURCE_PATH
            target.parent.mkdir(parents=True)
            target.write_bytes(self.source + b"changed")
            with self.assertRaises(ValueError):
                apply_patch(root)
            self.assertEqual(target.read_bytes(), self.source + b"changed")
            target.write_bytes(self.source)
            bad = root / "invalid.patch"
            bad.write_bytes(b"not a valid patch\n")
            with self.assertRaises(subprocess.CalledProcessError):
                apply_patch(root, bad)
            self.assertEqual(target.read_bytes(), self.source)

    def test_com_patch_checks_both_original_blobs_before_changing_either(self):
        sdk = Path(os.environ["SOFTING_SDK_ROOT"])
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            for path, sha in COM_BLOBS.items():
                target = root / path
                target.parent.mkdir(parents=True, exist_ok=True)
                if (sdk / ".git").exists():
                    data = subprocess.run(["git", "-C", str(sdk), "cat-file", "blob", sha],
                                          check=True, capture_output=True).stdout
                else:
                    data = (sdk / path).read_bytes()
                target.write_bytes(data)
            paths = list(COM_BLOBS)
            original = [(root / path).read_bytes() for path in paths]
            (root / paths[1]).write_bytes(original[1] + b"changed")
            with self.assertRaises(ValueError):
                apply_com_patch(root)
            self.assertEqual((root / paths[0]).read_bytes(), original[0])
            (root / paths[1]).write_bytes(original[1])
            apply_com_patch(root)
            self.assertTrue(all((root / path).read_bytes() != before
                                for path, before in zip(paths, original)))


if __name__ == "__main__":
    unittest.main()
