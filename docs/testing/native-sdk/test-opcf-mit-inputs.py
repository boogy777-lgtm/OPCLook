#!/usr/bin/env python3
"""Exercise provenance staging without a Windows compiler or OPC implementation."""
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("stage_opcf", Path(__file__).with_name("stage-opcf-mit-inputs.py"))
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)


def git(path, *args):
    return subprocess.run(["git", "-C", str(path), *args], check=True, capture_output=True).stdout.decode().strip()


class StagingTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.repo = self.root / "repo"
        self.repo.mkdir()
        git(self.repo, "init", "-q")
        git(self.repo, "config", "user.email", "fixture@example.invalid")
        git(self.repo, "config", "user.name", "fixture")
        (self.repo / "source.idl").write_bytes(b"OPC Foundation MIT License 1.00\noriginal\n")
        git(self.repo, "add", ".")
        git(self.repo, "commit", "-qm", "fixture")
        self.commit = git(self.repo, "rev-parse", "HEAD")
        self.blob = git(self.repo, "rev-parse", "HEAD:source.idl")
        self.files = {"file.idl": ("source.idl", self.blob)}
        self.output = self.root / "stage"

    def test_pinned_blob_used_despite_dirty_worktree(self):
        (self.repo / "source.idl").write_text("untrusted change")
        self.assertEqual(mod.stage(self.repo, self.output, self.commit, self.files), 1)
        self.assertEqual((self.output / "file.idl").read_bytes(), b"OPC Foundation MIT License 1.00\noriginal\n")
        with self.assertRaisesRegex(ValueError, "already exists"):
            mod.stage(self.repo, self.output, self.commit, self.files)

    def test_bad_head_blob_and_notice_fail_before_output(self):
        with self.assertRaisesRegex(ValueError, "HEAD mismatch"):
            mod.stage(self.repo, self.output, "0" * 40, self.files)
        with self.assertRaisesRegex(ValueError, "tree blob mismatch"):
            mod.stage(self.repo, self.output, self.commit, {"file.idl": ("source.idl", "0" * 40)})
        self.assertFalse(self.output.exists())
        (self.repo / "source.idl").write_text("not an MIT notice")
        git(self.repo, "add", ".")
        git(self.repo, "commit", "-qm", "no notice")
        head = git(self.repo, "rev-parse", "HEAD")
        blob = git(self.repo, "rev-parse", "HEAD:source.idl")
        with self.assertRaisesRegex(ValueError, "notice missing"):
            mod.stage(self.repo, self.output, head, {"file.idl": ("source.idl", blob)})
        self.assertFalse(self.output.exists())

    def test_no_directory_traversal(self):
        with self.assertRaisesRegex(ValueError, "invalid destination"):
            mod.stage(self.repo, self.output, self.commit, {"../bad": ("source.idl", self.blob)})
        self.assertFalse(self.output.exists())


if __name__ == "__main__":
    unittest.main()
