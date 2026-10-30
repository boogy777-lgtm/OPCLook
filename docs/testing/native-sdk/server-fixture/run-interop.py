#!/usr/bin/env python3
"""Run an owned local SDK server and fail on every interop/setup failure."""
import argparse
import os
from pathlib import Path
import subprocess
import tempfile
import time


def checked(command, timeout=20):
    print(f"fixture command: {command!r}", flush=True)
    result = subprocess.run(command, check=False, timeout=timeout)
    if result.returncode != 0:
        raise RuntimeError(f"fixture command failed: {result.returncode}")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--server", type=Path, required=True)
    parser.add_argument("--proxy", type=Path, required=True)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command:
        parser.error("an interop client command is required")
    server = str(args.server.resolve(strict=True))
    proxy = str(args.proxy.resolve(strict=True))
    regsvr32 = str(Path(os.environ["WINDIR"]) / "SysWOW64" / "regsvr32.exe")
    proxy_registered = False
    server_registered = False
    process = None
    cleanup_errors = []
    try:
        proxy_registered = True
        checked([regsvr32, "/s", proxy])
        server_registered = True
        checked([server, "--register"])
        with tempfile.TemporaryDirectory(prefix="opcl-da-fixture-") as directory:
            ready = Path(directory) / "ready"
            stop = Path(directory) / "stop"
            process = subprocess.Popen([server, str(ready), str(stop)])
            try:
                deadline = time.monotonic() + 15
                while not ready.exists():
                    if process.poll() is not None:
                        raise RuntimeError(f"server exited before ready: {process.returncode}")
                    if time.monotonic() >= deadline:
                        raise RuntimeError("server startup timed out")
                    time.sleep(0.05)
                checked(command, timeout=90)
            finally:
                stop.touch()
                try:
                    exit_code = process.wait(timeout=10)
                    if exit_code != 0:
                        cleanup_errors.append(f"server exit={exit_code}")
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=10)
                    cleanup_errors.append("server cleanup timed out")
    finally:
        for enabled, cmd in [(server_registered, [server, "--unregister"]),
                             (proxy_registered, [regsvr32, "/u", "/s", proxy])]:
            if enabled:
                try:
                    checked(cmd)
                except (RuntimeError, OSError, subprocess.TimeoutExpired) as error:
                    cleanup_errors.append(str(error))
        if cleanup_errors:
            raise RuntimeError("; ".join(cleanup_errors))
    print("Local DA COM fixture and cleanup: PASS", flush=True)


if __name__ == "__main__":
    main()
