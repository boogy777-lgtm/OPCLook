#!/usr/bin/env python3
"""Prepare an isolated pinned sample-server SDK for the R07 first-value probe.

This does not change Cargo's registry, this repository's dependency pins, or the
client SDK. Run against a disposable worktree and separate CARGO_TARGET_DIR.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

REVISION = "9ad28fc011002398f2e8a95696a50408a14531d9"
BLOB = "41ad79de03c1607a45db763e91c346cb14add7e6"
SOURCE = Path("src/subscriptions/session_subscriptions.rs")
BEFORE = "        let mut to_delete = Vec::new();\n        if self.subscriptions.is_empty() {"
AFTER = ("        let mut to_delete = Vec::new();\n"
         "        self.publish_request_queue.retain(|request| !request.response.is_closed());\n"
         "        if self.subscriptions.is_empty() {")


def git_blob(data: bytes) -> str:
    return hashlib.sha1(b"blob " + str(len(data)).encode("ascii") + b"\0" + data).hexdigest()


def transform(text: str) -> str:
    if text.count(BEFORE) != 1:
        raise ValueError("expected exactly one unpatched tick anchor")
    return text.replace(BEFORE, AFTER, 1)


def prepare(source: Path, dest: Path) -> None:
    if dest.exists():
        raise ValueError("destination already exists")
    info = json.loads((source / ".cargo_vcs_info.json").read_text(encoding="utf-8"))
    if info.get("git", {}).get("sha1") != REVISION or info.get("path_in_vcs") != "async-opcua-server":
        raise ValueError("server SDK provenance does not match the pinned revision")
    raw = (source / SOURCE).read_bytes()
    if git_blob(raw) != BLOB:
        raise ValueError("server SDK source blob does not match upstream pin")
    patched = transform(raw.decode("utf-8"))
    shutil.copytree(source, dest)
    (dest / SOURCE).write_text(patched, encoding="utf-8", newline="\n")


def baseline_reproduced(returncode: int, output: str) -> bool:
    return (returncode != 0 and "sdk_recovery_first_notification_after_reactivation ... FAILED" in output
            and "post-recovery events:" in output)


def patched_passed(returncode: int, output: str) -> bool:
    return returncode == 0 and "sdk_recovery_first_notification_after_reactivation ... ok" in output


class StandTests(unittest.TestCase):
    def test_transform_once_and_reject_changed_anchor(self):
        self.assertIn(AFTER, transform("prefix\n" + BEFORE + "\nsuffix"))
        with self.assertRaises(ValueError):
            transform("different")
        with self.assertRaises(ValueError):
            transform(BEFORE + "\n" + BEFORE)
        with self.assertRaises(ValueError):
            transform(AFTER)

    def test_outcome_classification_rejects_compile_or_unrelated_failure(self):
        expected = "post-recovery events: []\ntest x::sdk_recovery_first_notification_after_reactivation ... FAILED"
        self.assertTrue(baseline_reproduced(101, expected))
        self.assertFalse(baseline_reproduced(101, "error: could not compile"))
        self.assertFalse(baseline_reproduced(101, "test x::sdk_recovery_first_notification_after_reactivation ... FAILED"))
        self.assertFalse(baseline_reproduced(0, expected))
        self.assertTrue(patched_passed(0, "test x::sdk_recovery_first_notification_after_reactivation ... ok"))
        self.assertFalse(patched_passed(101, "test x::sdk_recovery_first_notification_after_reactivation ... ok"))

    def test_blob_hash_known_git_object(self):
        self.assertEqual(git_blob(b"hello\n"), "ce013625030ba8dba906f756967f9e9ca394464a")

    def test_prepare_enforces_pin_and_never_overwrites(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source, dest = root / "source", root / "output"
            (source / SOURCE.parent).mkdir(parents=True)
            (source / ".cargo_vcs_info.json").write_text(json.dumps({"git": {"sha1": REVISION}, "path_in_vcs": "async-opcua-server"}), encoding="utf-8")
            original = (BEFORE + "\n").encode()
            (source / SOURCE).write_bytes(original)
            with patch(__name__ + ".BLOB", git_blob(original)):
                prepare(source, dest)
                self.assertIn(AFTER, (dest / SOURCE).read_text())
                self.assertEqual((source / SOURCE).read_bytes(), original)
                with self.assertRaises(ValueError):
                    prepare(source, dest)
            with self.assertRaises(ValueError):
                prepare(source, root / "bad-source")
            (source / ".cargo_vcs_info.json").write_text(json.dumps({"git": {"sha1": "wrong"}, "path_in_vcs": "async-opcua-server"}), encoding="utf-8")
            with patch(__name__ + ".BLOB", git_blob(original)):
                with self.assertRaises(ValueError):
                    prepare(source, root / "bad-revision")
            self.assertFalse((root / "bad-revision").exists())


def run_stand(worktree: Path, server: Path, baseline_target: Path, patched_target: Path, logs: Path) -> None:
    """Run the strict ignored test in an explicitly disposable git worktree."""
    if not (worktree / ".git").is_file():
        raise ValueError("--worktree must be an existing linked worktree, not the primary checkout")
    if baseline_target.resolve() == patched_target.resolve():
        raise ValueError("baseline and patched CARGO_TARGET_DIR must be different")
    if logs.exists():
        raise ValueError("log directory already exists")
    status = subprocess.run(["git", "status", "--porcelain"], cwd=worktree, check=True, capture_output=True, text=True)
    if status.stdout.strip():
        raise ValueError("disposable worktree must start clean")
    if not server.is_dir():
        raise ValueError("prepared server directory does not exist")
    logs.mkdir(parents=True)
    head = subprocess.run(["git", "rev-parse", "HEAD"], cwd=worktree, check=True, capture_output=True, text=True).stdout.strip()
    strict_source = worktree / "crates/backend/src/bff/ua/live_tests.rs"
    (logs / "run.json").write_text(json.dumps({
        "worktree_head": head,
        "strict_test_file_git_blob": git_blob(strict_source.read_bytes()),
        "server_vcs": REVISION,
        "server_original_git_blob": BLOB,
        "server_patched_git_blob": git_blob((server / SOURCE).read_bytes()),
    }, indent=2) + "\n", encoding="utf-8")
    env = os.environ.copy()
    env.update({"CARGO_PROFILE_DEV_DEBUG": "0", "CARGO_PROFILE_TEST_DEBUG": "0", "CARGO_INCREMENTAL": "0", "CARGO_BUILD_JOBS": "2"})
    common = ["cargo", "test", "--offline", "--target", "x86_64-unknown-linux-gnu"]
    test = ["-p", "opclook-backend", "--lib", "sdk_recovery_first_notification_after_reactivation", "--", "--ignored", "--nocapture"]
    def execute(name: str, command: list[str], target: Path) -> subprocess.CompletedProcess[str]:
        env["CARGO_TARGET_DIR"] = str(target.resolve())
        result = subprocess.run(command, cwd=worktree, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, timeout=300)
        (logs / (name + ".log")).write_text(result.stdout, encoding="utf-8")
        return result
    before = execute("baseline", common + ["--locked"] + test, baseline_target)
    if not baseline_reproduced(before.returncode, before.stdout):
        raise ValueError("baseline did not reproduce the strict first-notification failure; inspect baseline.log")
    # Cargo updates only the disposable worktree's lockfile for this path override.
    config = 'patch.crates-io.async-opcua-server.path=' + json.dumps(str(server.resolve()))
    after = execute("patched", common + ["--config", config] + test, patched_target)
    if not patched_passed(after.returncode, after.stdout):
        raise ValueError("patched strict test did not pass; inspect patched.log")
    print(f"baseline: strict failure reproduced; patched: strict PASS; logs: {logs}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, help="installed locked async-opcua-server source")
    parser.add_argument("--dest", type=Path, help="new scratch directory for patched server")
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--run", action="store_true", help="run before/after in a clean disposable linked worktree")
    parser.add_argument("--worktree", type=Path)
    parser.add_argument("--baseline-target", type=Path)
    parser.add_argument("--patched-target", type=Path)
    parser.add_argument("--logs", type=Path)
    args = parser.parse_args()
    if args.self_test:
        return 0 if unittest.TextTestRunner().run(unittest.defaultTestLoader.loadTestsFromTestCase(StandTests)).wasSuccessful() else 1
    if args.source is None or args.dest is None:
        parser.error("--source and --dest are required unless --self-test is used")
    if args.run and any(value is None for value in (args.worktree, args.baseline_target, args.patched_target, args.logs)):
        parser.error("--run requires --worktree, --baseline-target, --patched-target and --logs")
    try:
        prepare(args.source, args.dest)
        if args.run:
            run_stand(args.worktree, args.dest, args.baseline_target, args.patched_target, args.logs)
    except (OSError, ValueError, UnicodeError, json.JSONDecodeError, subprocess.SubprocessError) as error:
        print(f"stand preparation failed: {error}", file=sys.stderr)
        return 1
    print(f"Prepared isolated patched server at {args.dest}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
