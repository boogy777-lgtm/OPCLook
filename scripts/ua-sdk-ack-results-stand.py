#!/usr/bin/env python3
"""Verify the isolated SDK ACK-result overlay and its counterfactual."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tomllib

HERE = Path(__file__).resolve().parent
DOCS = HERE.parent / 'docs' / 'testing'
REVISION = '9ad28fc011002398f2e8a95696a50408a14531d9'
OVERLAYS = ('ua-sdk-republish-v2-client.patch', 'ua-sdk-republish-fault-sticky.patch',
            'ua-sdk-readiness-client.patch', 'ua-sdk-readiness-racefix-client.patch',
            'ua-sdk-readiness-drain-client.patch', 'ua-sdk-ack-results-client.patch')
MARKER = '    state.ack_result_latest = Some(summary);'
PACKAGE_PREFIX = 'async-opcua-client-0.19.0/'
CARGO_METADATA = {'.cargo-ok', '.cargo-checksum.json'}


def check_paths(paths):
    """Reject equal or nested paths, resolving existing symlinks first."""
    resolved = {name: path.resolve() for name, path in paths.items()}
    for name, path in resolved.items():
        for other_name, other in resolved.items():
            if name < other_name and (path == other or path in other.parents or other in path.parents):
                raise ValueError(f'overlapping paths: {name} and {other_name}')
    return resolved


def archive_files(archive):
    """Authenticate the whole published archive and derive its file manifest."""
    lock = tomllib.loads((HERE.parent / 'Cargo.lock').read_text())
    packages = [p for p in lock['package'] if p.get('name') == 'async-opcua-client'
                and p.get('version') == '0.19.0' and p.get('source', '').startswith('registry+')]
    with archive.open('rb') as stream:
        archive_digest = hashlib.file_digest(stream, 'sha256').hexdigest()
    if len(packages) != 1 or archive_digest != packages[0].get('checksum'):
        raise ValueError('client archive checksum does not match Cargo.lock')
    files = {}
    with tarfile.open(archive, 'r:gz') as package:
        for entry in package:
            if not entry.isfile() or not entry.name.startswith(PACKAGE_PREFIX):
                raise ValueError('unexpected client archive member')
            name = entry.name[len(PACKAGE_PREFIX):]
            parts = name.split('/')
            if not name or any(part in ('', '.', '..') for part in parts) or name in files:
                raise ValueError('invalid or duplicate archive path')
            content = package.extractfile(entry)
            if content is None:
                raise ValueError('unreadable client archive member')
            files[name] = hashlib.file_digest(content, 'sha256').digest()
    if not files:
        raise ValueError('empty client archive')
    return files


def verify_source(source, expected):
    """Check all published files and reject extras or links before use."""
    directories = set()
    for name in expected:
        parent = Path(name).parent
        while parent != Path('.'):
            directories.add(parent.as_posix())
            parent = parent.parent
    observed = set()
    for root, dirs, files in os.walk(source, followlinks=False):
        root = Path(root)
        for name in dirs:
            path = root / name
            if path.is_symlink() or path.relative_to(source).as_posix() not in directories:
                raise ValueError('unexpected client source directory or symlink')
        for name in files:
            path = root / name
            relative = path.relative_to(source).as_posix()
            if path.is_symlink() or not path.is_file():
                raise ValueError('non-regular client source file')
            if relative in CARGO_METADATA:
                continue
            if relative not in expected:
                raise ValueError('unexpected client source file: ' + relative)
            with path.open('rb') as stream:
                digest = hashlib.file_digest(stream, 'sha256').digest()
            if digest != expected[relative]:
                raise ValueError('changed client source file: ' + relative)
            observed.add(relative)
    if observed != set(expected):
        raise ValueError('client source is missing published files')



def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('client-archive', 'client-source', 'client-dest', 'target-dir', 'log'):
        parser.add_argument('--' + name, required=True, type=Path)
    parser.add_argument('--offline', action='store_true', help='require a fully populated Cargo cache')
    args = parser.parse_args()
    paths = check_paths({'archive': args.client_archive, 'source': args.client_source,
                         'destination': args.client_dest, 'target': args.target_dir, 'log': args.log})
    if (args.client_dest.exists() or args.client_dest.is_symlink()
            or args.log.exists() or args.log.is_symlink()):
        raise ValueError('client destination and log must be new')
    source = paths['source']
    expected = archive_files(paths['archive'])
    verify_source(source, expected)
    info = json.loads((source / '.cargo_vcs_info.json').read_text())
    if info.get('git', {}).get('sha1') != REVISION or info.get('path_in_vcs') != 'async-opcua-client':
        raise ValueError('incorrect client VCS receipt')
    # Keep the drain runner's source checks and patch application as well.
    spec = importlib.util.spec_from_file_location('drain', HERE / 'ua-sdk-readiness-drain-stand.py')
    drain = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(drain)
    drain.check_files(source, drain.CLIENT_BLOBS)
    shutil.copytree(source, args.client_dest)
    client = paths['destination']
    verify_source(client, expected)
    for overlay in OVERLAYS:
        drain.apply(client, DOCS / overlay)
    # Seed compatible versions from this repository; Cargo adjusts its root
    # package for the standalone published client manifest.
    shutil.copyfile(HERE.parent / 'Cargo.lock', client / 'Cargo.lock')
    env = os.environ.copy()
    env.update(CARGO_TARGET_DIR=str(args.target_dir.resolve()), CARGO_PROFILE_DEV_DEBUG='0',
               CARGO_PROFILE_TEST_DEBUG='0', CARGO_INCREMENTAL='0', CARGO_BUILD_JOBS='2')
    base = ['cargo', 'test'] + (['--offline'] if args.offline else []) + ['--lib']
    service = client / 'src/session/services/subscriptions/service.rs'
    fixed = service.read_text()
    if fixed.count(MARKER) != 1:
        raise ValueError('counterfactual marker mismatch')
    try:
        service.write_text(fixed.replace(MARKER, '    let _ = summary;', 1))
        commands = [('counterfactual', 'response_results_do_not_hide_uncertainty_or_drop_callback', 0),
                    ('fixed', 'ack_results_regression', 3),
                    ('guard', 'publish_cancel_regression', 2),
                    ('recovery', 'recovery_', 16)]
        with args.log.open('x', encoding='utf-8') as log:
            for name, test_filter, expected_passes in commands:
                if expected_passes:
                    service.write_text(fixed)
                command = base + [test_filter, '--', '--nocapture']
                result = subprocess.run(command, cwd=client, env=env, text=True,
                                        stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
                log.write(f'=== {name} ===\n{result.stdout}\n')
                log.flush()
                print(f'{name}: exit {result.returncode}', flush=True)
                if expected_passes:
                    if result.returncode or f'test result: ok. {expected_passes} passed;' not in result.stdout:
                        raise RuntimeError(f'{name} did not pass tests; see log')
                elif (result.returncode == 0 or 'response summary' not in result.stdout
                      or 'test result: FAILED.' not in result.stdout):
                    raise RuntimeError('counterfactual did not fail at expected assertion; see log')
    finally:
        service.write_text(fixed)


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, RuntimeError, tarfile.TarError, subprocess.CalledProcessError) as error:
        print(f'ACK results stand: {error}', file=sys.stderr)
        sys.exit(1)
