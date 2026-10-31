#!/usr/bin/env python3
"""Fast, offline tests for SDK archive integrity and path isolation."""
import argparse
import importlib.util
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('stand', HERE / 'ua-sdk-ack-results-stand.py')
stand = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stand)


def rejected(fn):
    try:
        fn()
    except (ValueError, OSError):
        return
    raise AssertionError('expected rejection')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--client-source', type=Path, required=True)
    parser.add_argument('--client-archive', type=Path, required=True)
    args = parser.parse_args()
    expected = stand.archive_files(args.client_archive)
    stand.verify_source(args.client_source.resolve(), expected)
    with tempfile.TemporaryDirectory(prefix='ack-guards-') as temp:
        root = Path(temp)
        copied = root / 'source'
        shutil.copytree(args.client_source, copied)
        stand.verify_source(copied, expected)
        changed = copied / 'src/session/services/subscriptions/mod.rs'
        original = changed.read_bytes()
        changed.write_bytes(original + b'\n')
        rejected(lambda: stand.verify_source(copied, expected))
        changed.write_bytes(original)
        extra = copied / 'unexpected'
        extra.write_text('extra')
        rejected(lambda: stand.verify_source(copied, expected))
        extra.unlink()
        (copied / '.cargo-ok').write_text('')
        (copied / '.cargo-checksum.json').write_text('{}')
        stand.verify_source(copied, expected)
        extra.symlink_to(changed)
        rejected(lambda: stand.verify_source(copied, expected))
        extra.unlink()
        bad_archive = root / 'bad.crate'
        bad_archive.write_bytes(args.client_archive.read_bytes() + b'bad')
        rejected(lambda: stand.archive_files(bad_archive))
        names = ['archive', 'source', 'destination', 'target', 'log']
        separated = {name: root / name for name in names}
        stand.check_paths(separated)
        for i, name in enumerate(names):
            for other in names[i + 1:]:
                for parent, child in ((name, other), (other, name)):
                    paths = dict(separated)
                    paths[child] = paths[parent] / 'nested'
                    rejected(lambda: stand.check_paths(paths))
                paths = dict(separated)
                paths[other] = paths[name]
                rejected(lambda: stand.check_paths(paths))
        # Resolved symlinks cannot conceal overlap.
        link = root / 'link'
        link.symlink_to(copied, target_is_directory=True)
        rejected(lambda: stand.check_paths({'source': copied, 'target': link / 'nested'}))
        log = root / 'existing.log'
        log.write_text('preserve')
        dest = root / 'new-destination'
        command = [sys.executable, str(HERE / 'ua-sdk-ack-results-stand.py'),
                   '--client-source', str(copied), '--client-archive', str(args.client_archive),
                   '--client-dest', str(dest), '--target-dir', str(root / 'target'), '--log', str(log)]
        result = subprocess.run(command, capture_output=True, text=True)
        assert result.returncode and 'must be new' in result.stderr
        assert log.read_text() == 'preserve' and not dest.exists()
    print(f'archive/source verification and path guards passed ({len(expected)} published files)')


if __name__ == '__main__':
    main()
