#!/usr/bin/env python3
"""Run a secured SDK token renewal probe on isolated SDK copies."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

REVISION = '9ad28fc011002398f2e8a95696a50408a14531d9'
CLIENT_BLOBS = {
    'src/builder.rs': '26902438dc5096eecefd089448b7a50839c10266',
    'src/transport/channel.rs': 'bbce7617f2918a19110b9c0f2ffe85b5c15ccded',
    'src/lib.rs': '9b6fe1d2bf4aa9b279bf22b2377625f923aada59',
    'src/session/mod.rs': '1031d64b943900fd0e192c8f9375063b52a8ce89',
    'src/session/connect.rs': '6482789c73e6223fb290ca8871567c28d2ef9e20',
    'src/session/event_loop.rs': '5912a3579fe938fff7ca6b3daf68e56a6470da6d',
    'src/session/services/subscriptions/service.rs': '1fc634707f5be600f3c3aca5b2f02872c8cc3e46',
    'src/session/services/subscriptions/mod.rs': '704ed974f2b35387becf3e5a197b2d88ca4c9411',
    'src/session/services/subscriptions/state.rs': 'a2dd01f8468dcfd7a5f5c668596df1fe202adf2c',
}
SERVER_BLOBS = {
    'src/builder.rs': '3123626e8ca00ba19a721727a14bd6cf4ef8e843',
    'src/session/controller.rs': '61df6f670322883817469fdcbce8e30572fb48bf',
    'src/subscriptions/mod.rs': '618f5208abfb03eea34d9a2cac7d65b031eb9b75',
    'src/session/message_handler.rs': '4b05b0a2c14d7cb8ec2cd89887422fae96ab5455',
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
    for name in ('worktree', 'client-source', 'client-dest', 'server-source', 'server-dest', 'core-source', 'core-dest', 'target-dir', 'log'):
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
    server_info = json.loads((args.server_source / '.cargo_vcs_info.json').read_text())
    if server_info.get('git', {}).get('sha1') != REVISION or server_info.get('path_in_vcs') != 'async-opcua-server':
        raise ValueError('incorrect server VCS receipt')
    check_files(args.server_source, SERVER_BLOBS)
    if args.server_dest.exists():
        raise ValueError('server destination exists')
    core_info = json.loads((args.core_source / '.cargo_vcs_info.json').read_text())
    if core_info.get('git', {}).get('sha1') != REVISION or core_info.get('path_in_vcs') != 'async-opcua-core':
        raise ValueError('incorrect core VCS receipt')
    check_files(args.core_source, {'src/comms/secure_channel.rs': '587f80ace979608564846591e1ad241a238ccc56'})
    if args.core_dest.exists():
        raise ValueError('core destination exists')
    shutil.copytree(args.client_source, args.client_dest)
    shutil.copytree(args.server_source, args.server_dest)
    shutil.copytree(args.core_source, args.core_dest)
    docs = HERE.parent / 'docs/testing'
    apply(work, docs / 'ua-sdk-token-renewal-probe.patch')
    env = os.environ.copy()
    env.update(CARGO_TARGET_DIR=str(args.target_dir.resolve()), CARGO_PROFILE_DEV_DEBUG='0',
               CARGO_PROFILE_TEST_DEBUG='0', CARGO_INCREMENTAL='0', CARGO_BUILD_JOBS='2')
    server = 'patch.crates-io.async-opcua-server.path=' + json.dumps(str(args.server_dest.resolve()))
    client = 'patch.crates-io.async-opcua-client.path=' + json.dumps(str(args.client_dest.resolve()))
    core = 'patch.crates-io.async-opcua-core.path=' + json.dumps(str(args.core_dest.resolve()))
    command = ['cargo', 'test', '--offline', '--target', 'x86_64-unknown-linux-gnu',
               '--config', server, '--config', client, '--config', core, '-p', 'opclook-backend', '--lib',
               'sdk_secure_channel_token_renews_without_losing_session', '--', '--nocapture']
    result = subprocess.run(command, cwd=work, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    args.log.write_text(result.stdout, encoding='utf-8')
    print(result.stdout, end='')
    required = ['policy=Basic256Sha256 mode=SignAndEncrypt verify=true', 'same_session=true',
                'fresh=54.5 push=54.5', 'sdk_secure_channel_token_renews_without_losing_session ... ok']
    if result.returncode or not all(marker in result.stdout for marker in required):
        raise RuntimeError('secure token renewal probe failed or observations incomplete; see log')

if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f'boundary probe: {error}', file=sys.stderr)
        sys.exit(1)
