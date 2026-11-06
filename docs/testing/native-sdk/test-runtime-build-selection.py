"""Execute the actual build.rs: path/feature diagnostics only, not SDK ABI proof."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]


class RuntimeBuildSelection(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory(prefix="opclook-build-selection-")
        cls.binary = Path(cls.temp.name) / ("build-selection.exe" if os.name == "nt" else "build-selection")
        env = os.environ.copy()
        env["CARGO_MANIFEST_DIR"] = str(ROOT / "crates/backend")
        subprocess.run(["rustc", str(ROOT / "crates/backend/build.rs"), "-o", str(cls.binary)], env=env, check=True)

    @classmethod
    def tearDownClass(cls):
        cls.temp.cleanup()

    def run_build(self, hooks=False, override=None, target="windows"):
        env = os.environ.copy()
        for key in ("SOFTING_RUNTIME_LIB_DIR", "CARGO_FEATURE_NATIVE_DA_TEST_HOOKS"):
            env.pop(key, None)
        env["CARGO_CFG_TARGET_OS"] = target
        if hooks:
            env["CARGO_FEATURE_NATIVE_DA_TEST_HOOKS"] = "1"
        if override is not None:
            env["SOFTING_RUNTIME_LIB_DIR"] = override
        return subprocess.run([str(self.binary)], cwd=ROOT, env=env, text=True, capture_output=True, check=False)

    def test_hooks_require_explicit_nonempty_override(self):
        for override in (None, ""):
            with self.subTest(override=override):
                result = self.run_build(hooks=True, override=override)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("native-da-test-hooks requires SOFTING_RUNTIME_LIB_DIR", result.stderr)
                self.assertIn("hook-enabled", result.stderr)

    def test_production_defaults_to_vendored_no_hook_library(self):
        result = self.run_build()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("native/softing-da/lib/i686-pc-windows-msvc", result.stdout.replace("\\", "/"))

    def test_empty_production_override_preserves_missing_library_error(self):
        result = self.run_build(override="")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Missing Softing library:", result.stderr)

    def test_explicit_hook_directory_selects_only_that_directory(self):
        with tempfile.TemporaryDirectory(prefix="opclook-hook-path-") as directory:
            # Empty placeholders test selection/existence validation only.
            for name in ("softing_da_bridge.lib", "softing_expat.lib"):
                (Path(directory) / name).touch()
            result = self.run_build(hooks=True, override=directory)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("cargo:rustc-link-search=native=" + directory, result.stdout)
            self.assertNotIn("lib/i686-pc-windows-msvc", result.stdout.replace("\\", "/"))

    def test_non_windows_production_does_not_link_native_libraries(self):
        result = self.run_build(target="linux")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("cargo:rustc-link-lib", result.stdout)


if __name__ == "__main__":
    unittest.main()
