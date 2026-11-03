#!/usr/bin/env python3
"""Bind the SDK source audit to Cargo's installed, locked client crate."""
import argparse
import json
from pathlib import Path
import subprocess
import sys

VERSION = '0.19.0'
COMMIT = '9ad28fc011002398f2e8a95696a50408a14531d9'


def verify(metadata):
    packages = [p for p in metadata.get('packages', []) if p.get('name') == 'async-opcua-client']
    if len(packages) != 1 or packages[0].get('version') != VERSION:
        raise ValueError('expected exactly one async-opcua-client ' + VERSION)
    package = packages[0]
    if not str(package.get('source', '')).startswith('registry+'):
        raise ValueError('client is not the reviewed registry dependency')
    source = Path(package['manifest_path']).parent
    vcs = json.loads((source / '.cargo_vcs_info.json').read_text(encoding='utf-8'))
    if vcs.get('git', {}).get('sha1') != COMMIT or vcs.get('git', {}).get('dirty', False):
        raise ValueError('installed client VCS identity differs from audited source')
    return package.get('license'), COMMIT


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--metadata', type=Path, help='previously captured cargo metadata JSON')
    args = parser.parse_args()
    try:
        if args.metadata:
            metadata = json.loads(args.metadata.read_text(encoding='utf-8'))
        else:
            metadata = json.loads(subprocess.check_output(
                ['cargo', 'metadata', '--locked', '--format-version', '1'], text=True))
        license_expression, commit = verify(metadata)
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        print('UA SDK source identity FAILED: ' + str(error), file=sys.stderr)
        return 1
    print(f'UA SDK client {VERSION}: VCS {commit}; declared license {license_expression}; source identity only, not runtime acceptance')
    return 0


if __name__ == '__main__':
    sys.exit(main())
