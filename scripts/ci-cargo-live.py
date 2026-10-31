#!/usr/bin/env python3
"""Run a fixture Cargo test and reject a successful run that executed no tests."""
import re
import subprocess
import sys

command = sys.argv[1:]
if command[:1] == ['--']:
    command = command[1:]
if not command:
    raise SystemExit('usage: ci-cargo-live.py -- cargo test ...')
result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, errors='replace', timeout=80)
print(result.stdout, end='')
if result.returncode:
    raise SystemExit(result.returncode)
if not re.search(r'test result: ok\. [1-9][0-9]* passed;', result.stdout):
    raise SystemExit('fixture Cargo command did not report a passing test')
