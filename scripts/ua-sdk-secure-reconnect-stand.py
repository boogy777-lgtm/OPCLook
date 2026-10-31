#!/usr/bin/env python3
"""Run a secured SDK reconnection probe on isolated SDK copies."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

REVISION = '9ad28fc011002398f2e8a95696a50408a14531d9'
CLIENT_BLOBS = {
    'src/lib.rs': '9b6fe1d2bf4aa9b279bf22b2377625f923aada59',
    'src/session/mod.rs': '1031d64b943900fd0e192c8f9375063b52a8ce89',
    'src/session/connect.rs': '6482789c73e6223fb290ca8871567c28d2ef9e20',
    'src/session/event_loop.rs': '5912a3579fe938fff7ca6b3daf68e56a6470da6d',
    'src/session/services/subscriptions/service.rs': '1fc634707f5be600f3c3aca5b2f02872c8cc3e46',
    'src/session/services/subscriptions/state.rs': 'a2dd01f8468dcfd7a5f5c668596df1fe202adf2c',
}
WORK_BLOBS = {
    'crates/backend/src/bff/ua/live_tests.rs': 'ca034b47365a1027a7b705ec105b7a519c3f235e',
}
HERE = Path(__file__).resolve().parent

def blob(data):
    return hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()

def check_files(root, expected):
    for path, digest in expected.items():
        if blob((root / path).read_bytes()) != digest:
            raise ValueError('source blob differs: ' + path)

def apply(root, path):
    for args in (['--check'], []):
        subprocess.run(['git', 'apply', '--unidiff-zero', *args, str(path)], cwd=root, check=True)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('worktree', 'client-source', 'client-dest', 'server-source', 'server-dest', 'target-dir', 'log'):
        parser.add_argument('--' + name, required=True, type=Path)
    args = parser.parse_args()
    work = args.worktree.resolve()
    if not (work / '.git').is_file() or subprocess.check_output(['git', 'status', '--porcelain'], cwd=work, text=True):
        raise ValueError('worktree must be a clean disposable linked worktree')
    check_files(work, WORK_BLOBS)
    info = json.loads((args.client_source / '.cargo_vcs_info.json').read_text())
    if info.get('git', {}).get('sha1') != REVISION or info.get('path_in_vcs') != 'async-opcua-client':
        raise ValueError('incorrect client VCS receipt')
    check_files(args.client_source, CLIENT_BLOBS)
    if args.client_dest.exists():
        raise ValueError('client destination exists')
    spec = importlib.util.spec_from_file_location('fixture', HERE / 'ua-sdk-first-notification-stand.py')
    fixture = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(fixture)
    fixture.prepare(args.server_source, args.server_dest)
    shutil.copytree(args.client_source, args.client_dest)
    docs = HERE.parent / 'docs/testing'
    apply(args.client_dest, docs / 'ua-sdk-republish-v2-client.patch')
    apply(args.client_dest, docs / 'ua-sdk-republish-fault-sticky.patch')
    apply(work, docs / 'ua-sdk-secure-reconnect-probe.patch')
    env = os.environ.copy()
    env.update(CARGO_TARGET_DIR=str(args.target_dir.resolve()), CARGO_PROFILE_DEV_DEBUG='0',
               CARGO_PROFILE_TEST_DEBUG='0', CARGO_INCREMENTAL='0', CARGO_BUILD_JOBS='2')
    server = 'patch.crates-io.async-opcua-server.path=' + json.dumps(str(args.server_dest.resolve()))
    client = 'patch.crates-io.async-opcua-client.path=' + json.dumps(str(args.client_dest.resolve()))
    command = ['cargo', 'test', '--offline', '--target', 'x86_64-unknown-linux-gnu',
               '--config', server, '--config', client, '-p', 'opclook-backend', '--lib',
               'sdk_secure_channel_reactivation_preserves_unknown_history', '--', '--nocapture']
    result = subprocess.run(command, cwd=work, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    args.log.write_text(result.stdout, encoding='utf-8')
    print(result.stdout, end='')
    required = ['untrusted_self_signed_rejected=true', 'policy=Basic256Sha256 mode=SignAndEncrypt verify=true',
                'fresh=53.5 unknown=true', 'sdk_secure_channel_reactivation_preserves_unknown_history ... ok']
    if result.returncode or not all(marker in result.stdout for marker in required):
        raise RuntimeError('secure reconnect probe failed or observations incomplete; see log')

if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f'boundary probe: {error}', file=sys.stderr)
        sys.exit(1)
