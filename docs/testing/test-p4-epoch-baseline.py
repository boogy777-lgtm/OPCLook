#!/usr/bin/env python3
"""Check the counterfactual runner restores source even on execution failure."""
import contextlib
import importlib.util
import io
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("baseline", Path(__file__).with_name("p4-epoch-baseline.py"))
baseline = importlib.util.module_from_spec(spec)
spec.loader.exec_module(baseline)


class BaselineTests(unittest.TestCase):
    def test_restores_on_compile_failure_and_never_accepts_zero_tests(self):
        original = baseline.SOURCE.read_bytes()
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "table.rs"
            source.write_bytes(original)
            with patch.object(baseline, "SOURCE", source), patch.object(baseline, "run_tests", return_value=SimpleNamespace(returncode=101, stdout="compilation failed")):
                with self.assertRaises(RuntimeError):
                    with contextlib.redirect_stdout(io.StringIO()):
                        baseline.main()
            self.assertEqual(source.read_bytes(), original)

    def test_runner_requires_named_negative_and_positive_results(self):
        original = baseline.SOURCE.read_bytes()
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "table.rs"
            source.write_bytes(original)
            names = baseline.TESTS
            failed = "running 2 tests\n" + "\n".join(f"test app::tests::{name} ... FAILED" for name in names) + "\nassertion failed: test condition\nfailures:\n" + "\n".join(f"    app::tests::{name}" for name in names) + "\ntest result: FAILED. 0 passed; 2 failed\n"
            passed = "running 2 tests\n" + "\n".join(f"test app::tests::{name} ... ok" for name in names) + "\ntest result: ok. 2 passed\n"
            with patch.object(baseline, "SOURCE", source), patch.object(baseline, "run_tests", side_effect=[SimpleNamespace(returncode=101, stdout=failed), SimpleNamespace(returncode=0, stdout=passed)]):
                with contextlib.redirect_stdout(io.StringIO()):
                    baseline.main()
            self.assertEqual(source.read_bytes(), original)


if __name__ == "__main__":
    unittest.main()
