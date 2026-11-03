#!/usr/bin/env python3
import pathlib
import subprocess
import sys
import unittest

RUNNER = pathlib.Path(__file__).with_name("run-native-probe.py")


class NativeRunnerTest(unittest.TestCase):
    def run_probe(self, code, timeout="5"):
        return subprocess.run(
            [sys.executable, str(RUNNER), "--timeout", timeout, "--", sys.executable, "-c", code],
            capture_output=True, text=True, check=False,
        )

    def test_success_and_nonzero_are_reported(self):
        success = self.run_probe("print('fixture-output')")
        self.assertEqual(success.returncode, 0)
        self.assertIn("fixture-output", success.stdout)
        self.assertIn("native exit=0 hex=0x00000000", success.stdout)
        failure = self.run_probe("import sys; sys.exit(13)")
        self.assertNotEqual(failure.returncode, 0)
        self.assertIn("native exit=13 hex=0x0000000D", failure.stdout)

    def test_timeout_fails_visibly(self):
        result = self.run_probe("while True: pass", timeout="0.2")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("native probe timed out", result.stdout)


if __name__ == "__main__":
    unittest.main()
