#!/usr/bin/env python3
"""Counterfactual P3: disable only the column focus guard, then restore it."""
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'src/ui/dialogs/columns.rs'
TEST = 'p3_columns_focused_control_owns_enter_before_dialog_apply'
GUARD = b'if enter && !control_has_focus {'
DISABLED = b'if enter {'
ASSERTION = 'Enter must not also close the dialog'


def without_guard(original):
    if original.count(GUARD) != 1:
        raise ValueError('expected exactly one column focus guard')
    return original.replace(GUARD, DISABLED, 1)


def run_test():
    command = ['cargo', 'test', '--locked', '-p', 'opclook', '--test', 'ui_interactions',
               '--target', 'x86_64-unknown-linux-gnu', TEST, '--', '--nocapture', '--test-threads=1']
    result = subprocess.run(command, cwd=ROOT, env=dict(os.environ, CARGO_TERM_COLOR='never'),
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, errors='replace')
    print(result.stdout, flush=True)
    return result


def main():
    original = SOURCE.read_bytes()
    disabled = without_guard(original)
    try:
        SOURCE.write_bytes(disabled)
        result = run_test()
        if (result.returncode == 0 or 'running 1 test' not in result.stdout
                or 'test result: FAILED. 0 passed; 1 failed' not in result.stdout
                or 'test ' + TEST + ' ...' not in result.stdout
                or not re.search(r'(?m)^\s*' + re.escape(TEST) + r'\s*$', result.stdout.rsplit('failures:', 1)[-1])
                or 'panicked at' not in result.stdout or ASSERTION not in result.stdout):
            raise RuntimeError('disabled guard did not execute the named test and fail the expected assertion')
    finally:
        SOURCE.write_bytes(original)
    result = run_test()
    if (result.returncode != 0 or 'running 1 test' not in result.stdout
            or not re.search(r'(?m)^test ' + re.escape(TEST) + r' \.\.\. ok\s*$', result.stdout)
            or 'test result: ok. 1 passed; 0 failed' not in result.stdout):
        raise RuntimeError('restored guard did not execute and pass the named test')
    print('P3 counterfactual focus guard disabled: named assertion failed; restored: test passed')


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, RuntimeError) as error:
        print(f'P3 baseline verification FAILED: {error}', file=sys.stderr)
        sys.exit(1)
