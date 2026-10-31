#!/usr/bin/env python3
"""Run the transport probe in a disposable worktree, without editing Cargo's cache."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import shutil
import sys

REV = '9ad28fc011002398f2e8a95696a50408a14531d9'
BLOBS = {
    'src/session/services/subscriptions/state.rs': 'a2dd01f8468dcfd7a5f5c668596df1fe202adf2c',
    'src/session/connect.rs': '6482789c73e6223fb290ca8871567c28d2ef9e20',
    'src/session/services/subscriptions/service.rs': '1fc634707f5be600f3c3aca5b2f02872c8cc3e46',
}
TEST_BLOB = '7eee090282aea6f74ff14ddb81d59b658630a562'
HERE = Path(__file__).resolve().parent

def blob(data):
    return hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('worktree', 'client-source', 'server-source', 'server-dest', 'client-dest', 'target-dir', 'baseline-log', 'patched-log'):
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
    patch = HERE.parent / 'docs/testing/ua-sdk-republish-library-probe.patch'
    subprocess.run(['git', 'apply', '--check', str(patch)], cwd=work, check=True)
    subprocess.run(['git', 'apply', str(patch)], cwd=work, check=True)
    env = os.environ.copy()
    env.update(CARGO_TARGET_DIR=str(args.target_dir.resolve()), CARGO_PROFILE_DEV_DEBUG='0',
               CARGO_PROFILE_TEST_DEBUG='0', CARGO_INCREMENTAL='0', CARGO_BUILD_JOBS='2')
    if args.client_dest.exists():
        raise ValueError('client destination already exists')
    shutil.copytree(args.client_source, args.client_dest)
    sdk_patch = HERE.parent / 'docs/testing/ua-sdk-republish-library-client.patch'
    subprocess.run(['git', 'apply', '--unidiff-zero', '--check', str(sdk_patch)], cwd=args.client_dest, check=True)
    subprocess.run(['git', 'apply', '--unidiff-zero', str(sdk_patch)], cwd=args.client_dest, check=True)
    server_config = 'patch.crates-io.async-opcua-server.path=' + json.dumps(str(args.server_dest.resolve()))
    common = ['cargo', 'test', '--offline', '--target', 'x86_64-unknown-linux-gnu', '--config', server_config]
    tail = ['-p', 'opclook-backend', '--lib', 'sdk_republish_wire_loss_observation', '--', '--nocapture']
    def run(client, log):
        config = 'patch.crates-io.async-opcua-client.path=' + json.dumps(str(client.resolve()))
        result = subprocess.run(common + ['--config', config] + tail, cwd=work, env=env,
                                text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        log.write_text(result.stdout, encoding='utf-8')
        print(result.stdout, end='')
        return result
    baseline = run(args.client_source, args.baseline_log)
    if not (baseline.returncode != 0 and 'SDK did not automatically Republish' in baseline.stdout
            and 'sdk_republish_wire_loss_observation ... FAILED' in baseline.stdout):
        raise RuntimeError('baseline did not reproduce the expected failure; see log')
    patched = run(args.client_dest, args.patched_log)
    if not (patched.returncode == 0 and 'sdk_republish_callback=52.5 later=53.5 duplicates=0' in patched.stdout
            and 'sdk_republish_wire_loss_observation ... ok' in patched.stdout):
        raise RuntimeError('patched probe failed or observation incomplete; see log')

if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f'probe: {error}', file=sys.stderr)
        sys.exit(1)
