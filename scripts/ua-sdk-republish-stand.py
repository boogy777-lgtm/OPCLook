#!/usr/bin/env python3
"""Run the transport probe in a disposable worktree, without editing Cargo's cache."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys

REV = '9ad28fc011002398f2e8a95696a50408a14531d9'
BLOBS = {
    'src/session/connect.rs': '6482789c73e6223fb290ca8871567c28d2ef9e20',
    'src/session/services/subscriptions/service.rs': '1fc634707f5be600f3c3aca5b2f02872c8cc3e46',
}
TEST_BLOB = '7eee090282aea6f74ff14ddb81d59b658630a562'
HERE = Path(__file__).resolve().parent

def blob(data):
    return hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('worktree', 'client-source', 'server-source', 'server-dest', 'target-dir', 'log'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    work = args.worktree.resolve()
    if not (work / '.git').is_file():
        raise ValueError('worktree must be a disposable linked Git worktree')
    if subprocess.check_output(['git', 'status', '--porcelain'], cwd=work, text=True):
        raise ValueError('worktree must be clean')
    p = work / 'crates/backend/src/bff/ua/live_tests.rs'
    if blob(p.read_bytes()) != TEST_BLOB:
        raise ValueError('probe test source differs from its reviewed baseline')
    info = json.loads((args.client_source / '.cargo_vcs_info.json').read_text())
    if info.get('git', {}).get('sha1') != REV or info.get('path_in_vcs') != 'async-opcua-client':
        raise ValueError('wrong SDK client provenance')
    for path, expected in BLOBS.items():
        if blob((args.client_source / path).read_bytes()) != expected:
            raise ValueError('wrong SDK client source: ' + path)
    spec = importlib.util.spec_from_file_location('first', HERE / 'ua-sdk-first-notification-stand.py')
    first = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(first)
    first.prepare(args.server_source, args.server_dest)
    patch = HERE.parent / 'docs/testing/ua-sdk-republish-probe.patch'
    subprocess.run(['git', 'apply', '--check', str(patch)], cwd=work, check=True)
    subprocess.run(['git', 'apply', str(patch)], cwd=work, check=True)
    env = os.environ.copy()
    env.update(CARGO_TARGET_DIR=str(args.target_dir.resolve()), CARGO_PROFILE_DEV_DEBUG='0',
               CARGO_PROFILE_TEST_DEBUG='0', CARGO_INCREMENTAL='0', CARGO_BUILD_JOBS='2')
    config = 'patch.crates-io.async-opcua-server.path=' + json.dumps(str(args.server_dest.resolve()))
    client_config = 'patch.crates-io.async-opcua-client.path=' + json.dumps(str(args.client_source.resolve()))
    command = ['cargo', 'test', '--offline', '--target', 'x86_64-unknown-linux-gnu', '--config', config, '--config', client_config,
               '-p', 'opclook-backend', '--lib', 'sdk_republish_wire_loss_observation', '--', '--nocapture']
    result = subprocess.run(command, cwd=work, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    args.log.write_text(result.stdout, encoding='utf-8')
    print(result.stdout, end='')
    required = ('automatic_republish=0', 'manual_republish returned=52.5',
                'callback_after_manual_republish=none later_callback=53.5',
                'sdk_republish_wire_loss_observation ... ok')
    if result.returncode or not all(s in result.stdout for s in required):
        raise RuntimeError('probe failed or its observations were incomplete; see log')

if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f'probe: {error}', file=sys.stderr)
        sys.exit(1)
