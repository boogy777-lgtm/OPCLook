#!/usr/bin/env python3
"""POSIX launcher control/cleanup regression; does not simulate OPC or Windows events."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


class IsolationLauncherTest(unittest.TestCase):
    @unittest.skipIf(os.name == "nt", "POSIX executable fakes")
    def test_control_propagation_and_cleanup(self):
        for exit_code in (0, 7):
            with self.subTest(exit_code=exit_code), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                (root / "SysWOW64").mkdir()
                proxy = root / "proxy.dll"
                proxy.touch()
                log = root / "commands"
                common = "#!/usr/bin/env python3\nimport pathlib,sys,time,os\nlog=pathlib.Path(os.environ['FIXTURE_TEST_LOG'])\n"
                reg = root / "SysWOW64" / "regsvr32.exe"
                reg.write_text(common + "with log.open('a') as f:f.write('proxy '+repr(sys.argv[1:])+'\\n')\n")
                server = root / "server"
                server.write_text(common + "with log.open('a') as f:f.write('server '+repr(sys.argv[1:])+'\\n')\nif sys.argv[1].startswith('--'):sys.exit(0)\npathlib.Path(sys.argv[1]).touch()\nwhile not pathlib.Path(sys.argv[2]).exists():time.sleep(.01)\n")
                client = root / "client.py"
                client.write_text("import os,sys\np=os.environ['OPCLOOK_ISOLATION_PREFIX']\nassert p.startswith('Local\\\\OPCLookIsolation-')\nassert os.path.isfile(os.environ['OPCLOOK_ISOLATION_HELPER'])\nassert os.environ['OPCLOOK_ISOLATION_PYTHON']\nwith open(os.environ['FIXTURE_TEST_LOG'], 'a') as f:f.write('client '+p+'\\n')\nsys.exit(int(sys.argv[1]))\n")
                reg.chmod(0o755)
                server.chmod(0o755)
                run = subprocess.run([sys.executable, str(Path(__file__).with_name("run-interop.py")),
                                      "--server", str(server), "--proxy", str(proxy), "--isolation", "--",
                                      sys.executable, str(client), str(exit_code)],
                                     env=dict(os.environ, WINDIR=str(root), FIXTURE_TEST_LOG=str(log)),
                                     capture_output=True, text=True, timeout=10)
                self.assertEqual(run.returncode == 0, exit_code == 0, run.stdout + run.stderr)
                recorded = log.read_text()
                prefix = next(line[7:] for line in recorded.splitlines() if line.startswith("client "))
                self.assertIn(repr(prefix), recorded)
                self.assertIn("--unregister", recorded)
                self.assertIn("'/u'", recorded)


if __name__ == "__main__":
    unittest.main()
