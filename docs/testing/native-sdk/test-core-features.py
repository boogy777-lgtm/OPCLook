#!/usr/bin/env python3
"""Check test-only DCOM features against the exact public SDK config header.

This is a preprocessor test only; real Win32 compile/link/runtime belongs to CI.
"""
import hashlib
import os
from pathlib import Path
import subprocess
import sys
import tempfile

root = Path(os.environ["SOFTING_SDK_ROOT"])
config_path = "development/core/src/include/SOCmnCfg.h"
expected = "2cdc6df6bd87d390c5792f9451009ac13d9aeb09"
if (root / ".git").exists():
    source = subprocess.run(["git", "-C", str(root), "cat-file", "blob", expected],
                            check=True, capture_output=True).stdout
else:
    source = (root / config_path).read_bytes()
actual = hashlib.sha1(b"blob " + str(len(source)).encode() + b"\0" + source).hexdigest()
if actual != expected:
    raise SystemExit(f"SDK config mismatch: {actual}")
header = Path(__file__).with_name("core-probe").joinpath("stdafx.h").read_text()
prefix = header[:header.index('#include <windows.h>')]
start = header.index('// BEGIN N1 DCOM-ONLY FEATURES')
end = header.index('// END N1 DCOM-ONLY FEATURES', start)
feature_block = header[start:end]
program = "\n".join([
    "#define WIN32", prefix, '#include "SOCmnCfg.h"',
    "#if !defined(SOFEATURE_DCOM) || !defined(SOFEATURE_DA10)",
    '#error "SDK did not enable expected DA/DCOM features"', "#endif",
    feature_block, '#include "SOCmnCfg.h"',
    "#if !defined(SOFEATURE_DCOM) || !defined(SOFEATURE_DA10)",
    '#error "DA/DCOM unexpectedly disabled"', "#endif",
    "#if defined(SOFEATURE_SOAP) || defined(SOFEATURE_WATCH) || defined(SOFEATURE_XML_METHOD) || defined(SOFEATURE_CMN_XML)",
    '#error "SDK config reenabled excluded features"', "#endif", ""
])
with tempfile.TemporaryDirectory(prefix="softing-features-") as temp:
    Path(temp, "SOCmnCfg.h").write_bytes(source)
    subprocess.run([os.environ.get("CXX", "c++"), "-U__linux__", "-E", "-x", "c++",
                    "-I", temp, "-"], input=program, text=True, stdout=subprocess.DEVNULL,
                   check=True)
print("SDK DA/DCOM feature configuration preprocessor check passed")
