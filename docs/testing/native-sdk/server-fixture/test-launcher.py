#!/usr/bin/env python3
"""Launcher ownership tests with subprocess fakes, not OPC interop evidence."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


class LauncherTests(unittest.TestCase):
    def test_cleanup_after_client_failure_and_success(self):
        if os.name == "nt":
            self.skipTest("POSIX executable fakes; actual Windows launcher is the live gate")
        for code in (0, 7):
            with self.subTest(client_exit=code), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                system = root / "SysWOW64"
                system.mkdir()
                log = root / "commands"
                proxy = root / "proxy.dll"
                proxy.touch()
                common = "#!/usr/bin/env python3\nimport pathlib,sys,time,os\nlog=pathlib.Path(os.environ['FIXTURE_TEST_LOG'])\n"
                reg = system / "regsvr32.exe"
                reg.write_text(common + "with log.open('a') as f:f.write('proxy '+repr(sys.argv[1:])+'\\n')\n")
                server = root / "server"
                server.write_text(common + "with log.open('a') as f:f.write('server '+repr(sys.argv[1:])+'\\n')\nif sys.argv[1].startswith('--'):sys.exit(0)\npathlib.Path(sys.argv[1]).touch()\nwhile not pathlib.Path(sys.argv[2]).exists():time.sleep(.01)\n")
                reg.chmod(0o755)
                server.chmod(0o755)
                environment = dict(os.environ, WINDIR=str(root), FIXTURE_TEST_LOG=str(log))
                run = subprocess.run([sys.executable, str(Path(__file__).with_name("run-interop.py")),
                                      "--server", str(server), "--proxy", str(proxy), "--",
                                      sys.executable, "-c", f"raise SystemExit({code})"],
                                     env=environment, capture_output=True, text=True, timeout=10)
                self.assertEqual(run.returncode == 0, code == 0, run.stdout + run.stderr)
                recorded = log.read_text()
                self.assertIn("--register", recorded)
                self.assertIn("--unregister", recorded)
                self.assertIn("'/u'", recorded)


if __name__ == "__main__":
    unittest.main()
