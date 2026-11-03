#!/usr/bin/env python3
"""Exercise P3's guard restoration with a disposable source and fake Cargo."""
import contextlib
import importlib.util
import io
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('baseline', Path(__file__).with_name('p3-columns-baseline.py'))
baseline = importlib.util.module_from_spec(spec)
spec.loader.exec_module(baseline)


class BaselineTests(unittest.TestCase):
    def run_case(self, mode):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / 'src/ui/dialogs/columns.rs'
            source.parent.mkdir(parents=True)
            original = b'prefix\nif enter && !control_has_focus {\n  close = true;\n}\nsuffix\n'
            source.write_bytes(original)
            cargo = root / 'cargo'
            cargo.write_text('#!' + sys.executable + '\n' + '''
import os, pathlib, sys
text = pathlib.Path(os.environ['P3_TEST_SOURCE']).read_bytes()
mode = os.environ['P3_TEST_MODE']
if b'if enter && !control_has_focus {' not in text:
    if mode == 'compile': print('error: compile failed'); sys.exit(101)
    if mode == 'zero': print('running 0 tests\\ntest result: ok. 0 passed; 0 failed'); sys.exit(0)
    print('running 1 test\\ntest p3_columns_focused_control_owns_enter_before_dialog_apply ... FAILED')
    print('thread panicked at test: Enter must not also close the dialog' if mode != 'wrong' else 'thread panicked at unrelated condition')
    print('failures:\\n    p3_columns_focused_control_owns_enter_before_dialog_apply\\ntest result: FAILED. 0 passed; 1 failed'); sys.exit(101)
print('running 1 test\\ntest p3_columns_focused_control_owns_enter_before_dialog_apply ... ok')
print('test result: ok. 1 passed; 0 failed')
''')
            cargo.chmod(0o755)
            env = {'PATH': str(root) + os.pathsep + os.environ.get('PATH', ''),
                   'P3_TEST_SOURCE': str(source), 'P3_TEST_MODE': mode}
            with patch.object(baseline, 'ROOT', root), patch.object(baseline, 'SOURCE', source), patch.dict(os.environ, env), contextlib.redirect_stdout(io.StringIO()):
                if mode == 'ok':
                    baseline.main()
                else:
                    with self.assertRaises(RuntimeError):
                        baseline.main()
            self.assertEqual(source.read_bytes(), original)

    def test_named_failure_then_restored_pass(self):
        self.run_case('ok')

    def test_compile_zero_and_wrong_assertion_are_rejected_and_restored(self):
        for mode in ('compile', 'zero', 'wrong'):
            with self.subTest(mode=mode):
                self.run_case(mode)

    def test_guard_missing_or_repeated_is_rejected(self):
        with self.assertRaises(ValueError):
            baseline.without_guard(b'if enter {')
        with self.assertRaises(ValueError):
            baseline.without_guard(baseline.GUARD + baseline.GUARD)


if __name__ == '__main__':
    unittest.main()
