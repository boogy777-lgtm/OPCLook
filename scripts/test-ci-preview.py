#!/usr/bin/env python3
"""Offline checks for CI's packaged CLI and tooltip evidence helpers."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parent

class PreviewHelpersTests(unittest.TestCase):
    def test_cli_reads_and_rejects_bad_value_or_failed_exit(self):
        with tempfile.TemporaryDirectory() as tmp:
            exe = Path(tmp) / 'fake-cli'
            # POSIX fixture exercises the subprocess and JSON boundary; Windows
            # executes the actual packaged binary in the integration workflow.
            def execute(value, status=0):
                record = {'node_id': 'Fixture.Value', 'data': {
                    'value': {'type': 'i64', 'value': value},
                    'quality': {'kind': 'good'}, 'reason': None}}
                exe.write_text('#!' + sys.executable + '\nimport json, sys\nprint(' + repr(json.dumps(record)) + ')\nsys.exit(' + str(status) + ')\n')
                exe.chmod(0o755)
                return subprocess.run([sys.executable, str(SCRIPTS / 'ci-preview-cli.py'), str(exe)], capture_output=True)
            self.assertEqual(execute('42').returncode, 0)
            self.assertNotEqual(execute('43').returncode, 0)
            self.assertNotEqual(execute('42', 5).returncode, 0)

    def test_live_wrapper_rejects_zero_or_failing_tests(self):
        def execute(output, status=0):
            program = "import sys; print(" + repr(output) + "); sys.exit(" + str(status) + ")"
            return subprocess.run([sys.executable, str(SCRIPTS / 'ci-cargo-live.py'), '--', sys.executable, '-c', program], capture_output=True)
        self.assertEqual(execute('test result: ok. 1 passed; 0 failed;').returncode, 0)
        self.assertNotEqual(execute('test result: ok. 0 passed; 0 failed;').returncode, 0)
        self.assertNotEqual(execute('test result: ok. 1 passed; 0 failed;', 1).returncode, 0)

    def test_png_helper_checks_presence_and_signature(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            folder = root / 'docs/screenshots'
            folder.mkdir(parents=True)
            path = folder / 't08-value-time-tooltip-ru.png'
            def execute():
                return subprocess.run([sys.executable, str(SCRIPTS / 'ci-t08-png.py')], cwd=root, capture_output=True)
            self.assertNotEqual(execute().returncode, 0)
            path.write_bytes(b'not png')
            self.assertNotEqual(execute().returncode, 0)
            path.write_bytes(b'\x89PNG\r\n\x1a\ncontents')
            result = execute()
            self.assertEqual(result.returncode, 0)
            self.assertIn(b'T08_PNG_CHUNK ', result.stdout)

if __name__ == '__main__':
    unittest.main()
