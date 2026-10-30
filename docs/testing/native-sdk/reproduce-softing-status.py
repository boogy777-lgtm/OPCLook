#!/usr/bin/env python3
"""Compile the exact SDK callback method in a small, isolated fake environment.

This exercises the actual method body, not a copied algorithm; it does not link
or test the full SDK, COM marshalling, callback lifetime, or a live server.
"""
import hashlib
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from softing_patch import apply_patch

root = Path(os.environ["SOFTING_SDK_ROOT"])
subprocess.run([sys.executable, str(Path(__file__).with_name("verify-softing-sdk.py"))], check=True)
source_path = root / "development/core/src/SODaC/SODaCCallback.cpp"
# A Windows checkout may normalize line endings. Read the exact pinned blob
# from Git when available; an unpacked source tree must already match its SHA.
if (root / ".git").exists():
    source = subprocess.run(["git", "-C", str(root), "cat-file", "blob",
                             "59205bb9b6e32641c966d334589fe355bafe273f"],
                            check=True, capture_output=True).stdout
else:
    source = source_path.read_bytes()
blob = hashlib.sha1(b"blob " + str(len(source)).encode() + b"\0" + source).hexdigest()
expected = "59205bb9b6e32641c966d334589fe355bafe273f"
if blob != expected:
    raise SystemExit(f"SDK source mismatch: expected {expected}, got {blob}")
arguments = sys.argv[1:]
patched = "--patched" in arguments
if patched:
    arguments.remove("--patched")
    with tempfile.TemporaryDirectory(prefix="softing-patch-") as patch_dir:
        target = Path(patch_dir) / "development/core/src/SODaC/SODaCCallback.cpp"
        target.parent.mkdir(parents=True)
        target.write_bytes(source)
        apply_patch(Path(patch_dir))
        source = target.read_bytes()
text = source.decode("utf-8")
start = text.index("HRESULT SODaCDataCallback::handleReadAdvise(")
end = text.index("} // handleReadAdvise", start) + 1
method = text[start:end]
write_start = text.index("STDMETHODIMP SODaCDataCallback::OnWriteComplete(")
loop_start = text.index("\t\tfor (i = 0; i < dwCount; i++)", write_start)
call = text.index("\t\tm_group->onWriteAdvise(", loop_start)
loop_end = text.index(";", call) + 1
write_fragment = text[loop_start:loop_end]
template = Path(__file__).with_name("softing-callback-harness.cpp").read_text()
with tempfile.TemporaryDirectory(prefix="softing-status-probe-") as temp:
    cpp = Path(temp) / "actual-sdk-method.cpp"
    binary = Path(temp) / "probe"
    cpp.write_text(template.replace("@SDK_METHOD@", method).replace("@SDK_WRITE_FRAGMENT@", write_fragment))
    if os.environ.get("SDK_CXX_STYLE") == "msvc":
        binary = binary.with_suffix(".exe")
        command = ["cl", "/nologo", "/std:c++14", "/EHsc", str(cpp), f"/Fe:{binary}"]
    else:
        command = [os.environ.get("CXX", "c++"), "-std=c++11", "-Wall", "-Wextra",
                   str(cpp), "-o", str(binary)]
    subprocess.run(command, cwd=temp, check=True)
    subprocess.run([str(binary), *arguments], check=True)
