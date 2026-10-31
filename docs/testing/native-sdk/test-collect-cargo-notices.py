#!/usr/bin/env python3
import importlib.util
import hashlib
import json
import tarfile
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("collector", Path(__file__).with_name("collect-cargo-notices.py"))
collector = importlib.util.module_from_spec(spec)
spec.loader.exec_module(collector)


class CargoNoticeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.workspace = self.root / "workspace"
        self.workspace.mkdir()
        (self.workspace / "LICENSE").write_bytes(b"workspace license\r\n")
        (self.workspace / "Cargo.lock").write_text('''version = 4
[[package]]
name = "opclook-cli"
version = "1.0"
[[package]]
name = "dependency"
version = "2.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
''')
        cli = self.workspace / "cli"
        cli.mkdir()
        (cli / "Cargo.toml").write_text('[package]\nname="opclook-cli"\nversion="1.0"\n')
        self.home = self.root / "cargo"
        self.pkg = self.home / "registry/src/index-test/dependency-2.0"
        self.pkg.mkdir(parents=True)
        (self.pkg / "Cargo.toml").write_text('[package]\nname="dependency"\nversion="2.0"\n')
        self.rows = [
            {"id": "workspace-cli", "name": "opclook-cli", "version": "1.0", "source": None,
             "license": "MIT", "license_file": None, "manifest_path": str(cli / "Cargo.toml")},
            {"id": "registry-dependency", "name": "dependency", "version": "2.0",
             "source": collector.REGISTRY_SOURCE, "license": "MIT OR Apache-2.0", "license_file": None,
             "manifest_path": str(self.pkg / "Cargo.toml")},
        ]
        self.metadata = {"workspace_root": str(self.workspace), "workspace_members": ["workspace-cli"]}
        self.output = self.root / "output"

    def run_collect(self):
        pkg_id = self.rows[1]["name"] + "-" + self.rows[1]["version"]
        archive = self.home / "registry/cache/index-test" / (pkg_id + ".crate")
        archive.parent.mkdir(parents=True, exist_ok=True)
        with tarfile.open(archive, "w:gz") as tar:
            for path in sorted(self.pkg.iterdir()):
                tar.add(path, arcname=pkg_id + "/" + path.name, recursive=False)
        checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
        lock = self.workspace / "Cargo.lock"
        import re
        lock.write_text(re.sub(r'checksum = "[0-9a-f]{64}"', f'checksum = "{checksum}"', lock.read_text()))
        return collector.collect(self.metadata, self.rows, self.workspace, self.home, self.output)

    def make_curated(self):
        self.pkg.rename(self.pkg.parent / "async-opcua-0.19.0")
        self.pkg = self.pkg.parent / "async-opcua-0.19.0"
        (self.pkg / "Cargo.toml").write_text('[package]\nname="async-opcua"\nversion="0.19.0"\nlicense="MPL-2.0"\nrepository="https://github.com/freeopcua/async-opcua"\n')
        (self.pkg / ".cargo_vcs_info.json").write_text(json.dumps({"git": {"sha1": collector.UPSTREAM_REVISION}, "path_in_vcs": "async-opcua"}))
        row = self.rows[1]
        row.update(name="async-opcua", version="0.19.0", license="MPL-2.0", manifest_path=str(self.pkg / "Cargo.toml"))
        lock = self.workspace / "Cargo.lock"
        lock.write_text(lock.read_text().replace('name = "dependency"\nversion = "2.0"', 'name = "async-opcua"\nversion = "0.19.0"'))

    def test_curated_upstream_only_after_archive_vcs_verification(self):
        self.make_curated()
        result = self.run_collect()
        self.assertEqual(result["missing"], [])
        pkg = result["packages"][1]
        self.assertTrue(pkg["upstream_license"]["vcs_and_manifest_archive_verified"])
        self.assertEqual(pkg["files"][0]["source_kind"], "verified-upstream-root-license")
        self.assertIsNone(pkg["files"][0]["archive_member_verified"])
        self.assertEqual(pkg["files"][0]["sha256"], collector.UPSTREAM_LICENSE_SHA256)
        vcs = self.pkg / ".cargo_vcs_info.json"
        vcs.write_text(vcs.read_text().replace(collector.UPSTREAM_REVISION, "0" * 40))
        with self.assertRaisesRegex(ValueError, "differs from locked registry archive"):
            # A valid-looking changed receipt cannot be substituted after archive verification.
            vcs.write_text(vcs.read_text().replace("0" * 40, collector.UPSTREAM_REVISION))
            (self.pkg / "Cargo.toml").write_text((self.pkg / "Cargo.toml").read_text() + "\n# changed\n")
            collector.collect(self.metadata, self.rows, self.workspace, self.home, self.root / "tampered")

    def test_curated_wrong_revision_stays_missing(self):
        self.make_curated()
        vcs = self.pkg / ".cargo_vcs_info.json"
        vcs.write_text(vcs.read_text().replace(collector.UPSTREAM_REVISION, "0" * 40))
        result = self.run_collect()
        self.assertNotIn("upstream_license", result["packages"][1])
        self.assertIn("no package license/notice text found", result["missing"][0]["reasons"])

    def test_curated_license_wrong_hash_rejected_and_crlf_reconstructed(self):
        self.make_curated()
        original = collector.UPSTREAM_LICENSE.read_bytes()
        read_bytes = Path.read_bytes
        def contents(path):
            if path == collector.UPSTREAM_LICENSE:
                return original.replace(b"\n", b"\r\n")
            return read_bytes(path)
        with patch.object(Path, "read_bytes", contents):
            self.run_collect()
        self.assertEqual((self.output / "files" / collector.UPSTREAM_LICENSE_SHA256).read_bytes(), original)
        def corrupt(path):
            if path == collector.UPSTREAM_LICENSE:
                return original + b"tampered"
            return read_bytes(path)
        with patch.object(Path, "read_bytes", corrupt), self.assertRaisesRegex(ValueError, "does not match pinned source"):
            collector.collect(self.metadata, self.rows, self.workspace, self.home, self.root / "corrupt")

    def test_exact_bytes_checksum_dedup_and_workspace_fallback(self):
        (self.pkg / "LICENSE-MIT").write_bytes(b"test license\r\n")
        (self.pkg / "NOTICE").write_bytes(b"test license\r\n")
        result = self.run_collect()
        self.assertEqual(result["missing"], [])
        self.assertEqual(result["unique_files"], 2)
        self.assertTrue(result["packages"][1]["archive_verified"])
        self.assertEqual(len(result["packages"][1]["lock_checksum_sha256"]), 64)
        files = result["packages"][1]["files"]
        self.assertEqual(files[0]["sha256"], files[1]["sha256"])
        self.assertTrue(files[0]["archive_member_verified"])
        self.assertEqual((self.output / "files" / files[0]["sha256"]).read_bytes(), b"test license\r\n")
        self.assertEqual(json.loads((self.output / "manifest.json").read_text())["packages"], result["packages"])
        self.assertEqual(result["packages"][0]["files"][0]["path"], "<workspace>/LICENSE")

    def test_manifest_is_repeatable_from_identical_locked_inputs(self):
        (self.pkg / "LICENSE").write_text("license")
        first = self.run_collect()
        second = collector.collect(self.metadata, self.rows, self.workspace, self.home, self.root / "second")
        self.assertEqual(first, second)
        self.assertEqual((self.output / "manifest.json").read_bytes(), (self.root / "second/manifest.json").read_bytes())

    def test_packager_glue_stores_manifest_and_missing_summary(self):
        self.run_collect()  # Prepare a locked archive with no license text.
        spec = importlib.util.spec_from_file_location("preview", Path(__file__).with_name("package-preview.py"))
        preview = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(preview)
        bundle = self.root / "bundle"
        (bundle / "evidence").mkdir(parents=True)
        result = preview.collect_notice_evidence(bundle, self.metadata, self.rows, self.workspace, self.home)
        summary = json.loads((bundle / "evidence/cargo-notice-summary.json").read_text())
        self.assertEqual(summary["package_count"], 2)
        self.assertEqual(summary["missing"], result["missing"])
        self.assertTrue((bundle / "notices/cargo/manifest.json").is_file())

    def test_missing_and_missing_declared_are_explicit(self):
        self.rows[1]["license_file"] = "COPYING"
        result = self.run_collect()
        self.assertEqual(len(result["missing"]), 1)
        self.assertIn("declared license_file is absent", result["missing"][0]["reasons"])
        self.assertIn("no package license/notice text found", result["missing"][0]["reasons"])

    def test_rejects_parent_and_symlink_escape(self):
        self.rows[1]["license_file"] = "../other/LICENSE"
        with self.assertRaisesRegex(ValueError, "escapes package root"):
            self.run_collect()
        self.rows[1]["license_file"] = None
        self.output = self.root / "second-output"
        (self.pkg / "LICENSE").symlink_to(self.workspace / "LICENSE")
        with self.assertRaisesRegex(ValueError, "symlink escapes"):
            self.run_collect()

    def test_rejects_ambiguous_case_independent_of_filesystem(self):
        # One real file is valid on both case-sensitive and case-insensitive
        # filesystems. Only directory enumeration is simulated for the negative
        # case: it presents two spellings of the same candidate name.
        (self.pkg / "LICENSE").write_text("one")
        valid = self.run_collect()
        self.assertEqual(valid["missing"], [])
        self.assertEqual(valid["packages"][1]["files"][0]["path"], "LICENSE")
        real_iterdir = Path.iterdir

        def ambiguous_iterdir(path):
            entries = list(real_iterdir(path))
            if path == self.pkg:
                entries.append(path / "license")
            return iter(entries)

        with patch.object(Path, "iterdir", ambiguous_iterdir):
            with self.assertRaisesRegex(ValueError, "ambiguous case-insensitive notice name"):
                collector.collect(self.metadata, self.rows, self.workspace, self.home,
                                  self.root / "ambiguous-output")

    def test_rejects_untrusted_manifest(self):
        self.rows[1]["manifest_path"] = str(self.workspace / "cli/Cargo.toml")
        with self.assertRaisesRegex(ValueError, "outside trusted package root"):
            self.run_collect()

    def test_tampered_installed_text_and_missing_archive_are_explicit(self):
        (self.pkg / "LICENSE").write_bytes(b"authentic")
        self.run_collect()
        (self.pkg / "LICENSE").write_bytes(b"modified")
        with self.assertRaisesRegex(ValueError, "differs from locked registry archive"):
            collector.collect(self.metadata, self.rows, self.workspace, self.home, self.root / "tampered-output")
        archive = self.home / "registry/cache/index-test/dependency-2.0.crate"
        archive.unlink()
        result = collector.collect(self.metadata, self.rows, self.workspace, self.home, self.root / "missing-archive-output")
        self.assertFalse(result["packages"][1]["archive_verified"])
        self.assertIn("locked registry archive unavailable or exceeds limit", result["missing"][0]["reasons"])

    def test_archive_checksum_mismatch_and_duplicate_members_are_rejected(self):
        (self.pkg / "LICENSE").write_bytes(b"authentic")
        self.run_collect()
        archive = self.home / "registry/cache/index-test/dependency-2.0.crate"
        archive.write_bytes(archive.read_bytes() + b"modified")
        with self.assertRaisesRegex(ValueError, "checksum mismatch"):
            collector.collect(self.metadata, self.rows, self.workspace, self.home, self.root / "checksum-output")
        with tarfile.open(archive, "w:gz") as tar:
            tar.add(self.pkg / "LICENSE", arcname="dependency-2.0/LICENSE")
            tar.add(self.pkg / "LICENSE", arcname="dependency-2.0/LICENSE")
        lock = self.workspace / "Cargo.lock"
        import re
        checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
        lock.write_text(re.sub(r'checksum = "[0-9a-f]{64}"', f'checksum = "{checksum}"', lock.read_text()))
        with self.assertRaisesRegex(ValueError, "duplicate notice member"):
            collector.collect(self.metadata, self.rows, self.workspace, self.home, self.root / "duplicate-output")

    def test_missing_license_expression_and_oversize_are_reported(self):
        self.rows[1]["license"] = None
        (self.pkg / "LICENSE").write_bytes(b"x" * (collector.MAX_FILE_BYTES + 1))
        reasons = self.run_collect()["missing"][0]["reasons"]
        self.assertIn("no declared license expression", reasons)
        self.assertIn("notice exceeds per-file limit: LICENSE", reasons)
        self.assertIn("no package license/notice text found", reasons)


if __name__ == "__main__":
    unittest.main()
