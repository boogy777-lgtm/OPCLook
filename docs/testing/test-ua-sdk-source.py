#!/usr/bin/env python3
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('guard', Path(__file__).with_name('ua-sdk-source.py'))
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)


class InstalledSource(unittest.TestCase):
    def test_installed_vcs_and_registry_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            vcs = root / '.cargo_vcs_info.json'
            vcs.write_text(json.dumps({'git': {'sha1': guard.COMMIT}}))
            package = {'name': 'async-opcua-client', 'version': guard.VERSION,
                       'source': 'registry+https://github.com/rust-lang/crates.io-index',
                       'manifest_path': str(root / 'Cargo.toml'), 'license': 'MPL-2.0'}
            metadata = {'packages': [package]}
            self.assertEqual(guard.verify(metadata), ('MPL-2.0', guard.COMMIT))
            for change in [{'version': '0.18.0'}, {'source': None}]:
                with self.assertRaises(ValueError):
                    guard.verify({'packages': [dict(package, **change)]})
            with self.assertRaises(ValueError):
                guard.verify({'packages': [package, package]})
            for git in [{'sha1': '0' * 40}, {'sha1': guard.COMMIT, 'dirty': True}]:
                vcs.write_text(json.dumps({'git': git}))
                with self.assertRaises(ValueError):
                    guard.verify(metadata)
            vcs.unlink()
            with self.assertRaises(OSError):
                guard.verify(metadata)


if __name__ == '__main__':
    unittest.main()
