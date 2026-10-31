#!/usr/bin/env python3
"""Run the test-only Engine/Gateway replay boundary on isolated SDK copies."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

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
    'crates/backend/src/bff/ua/worker.rs': '8284344cf98dd605cebf4c999de97f0f0f4cfdec',
    'crates/backend/src/bff/ua/session/connection.rs': '6d09a120178c226a2af96200c3776354bec8c24a',
    'crates/backend/src/bff/ua/session/core.rs': '2b830a8f7a5a8fb2929a5dcdec95370dfbb9c559',
    'crates/backend/src/bff/ua/session/mod.rs': 'bf90d0ebde4d2e7210249d2861c890413a983654',
    'crates/backend/Cargo.toml': '466a16a57182498ad44cd5078f314c6425ef3202',
    'crates/backend/src/bff/ua/session/subscription.rs': '5abdf4cfa456299bd811a658c922e4a92c12e5fd',
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
    if args.log.exists():
        raise ValueError('log destination exists')
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
    apply(args.client_dest, docs / 'ua-sdk-readiness-client.patch')
    apply(args.client_dest, docs / 'ua-sdk-readiness-racefix-client.patch')
    apply(args.client_dest, docs / 'ua-sdk-readiness-drain-client.patch')
    apply(work, docs / 'ua-sdk-readiness-drain-engine.patch')
    env = os.environ.copy()
    env.update(CARGO_TARGET_DIR=str(args.target_dir.resolve()), CARGO_PROFILE_DEV_DEBUG='0',
               CARGO_PROFILE_TEST_DEBUG='0', CARGO_INCREMENTAL='0', CARGO_BUILD_JOBS='2')
    with tempfile.TemporaryDirectory(prefix='opclook-replay-marker-') as temp:
        marker = Path(temp) / 'closed-gate-marker'
        env['OPCLOOK_TEST_REPLAY_BOUNDARY'] = str(marker)
        server = 'patch.crates-io.async-opcua-server.path=' + json.dumps(str(args.server_dest.resolve()))
        client = 'patch.crates-io.async-opcua-client.path=' + json.dumps(str(args.client_dest.resolve()))
        command = ['cargo', 'test', '--offline', '--target', 'x86_64-unknown-linux-gnu',
                   '--config', server, '--config', client, '-p', 'opclook-backend', '--lib',
                   'sdk_replay_engine_snapshot_boundary', '--', '--nocapture']
        result = subprocess.run(command, cwd=work, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        with args.log.open('x', encoding='utf-8') as log:
            log.write(result.stdout)
        print(result.stdout, end='')
        if (result.returncode or 'sdk_replay_engine_snapshot_boundary ... ok' not in result.stdout
            or 'sdk_52_gate_drop=true engine_snapshot=53.5 push=54.5 history_unknown=true' not in result.stdout
            or 'PROBE drained gen=1 outcome=Complete' not in result.stdout
            or 'PROBE ack rev=2 gen=1 confirmed=true' not in result.stdout
            or not marker.is_file()):
            raise RuntimeError('boundary probe failed or omitted its marker; see log')

if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f'boundary probe: {error}', file=sys.stderr)
        sys.exit(1)
