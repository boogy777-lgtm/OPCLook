#!/usr/bin/env python3
"""Verify generation orchestration and input guards, not Microsoft MIDL output."""
import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("gen", Path(__file__).with_name("generate-opcf-midl.py"))
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)


class GenerationTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.source = self.root / "source"
        self.source.mkdir()
        self.output = self.root / "out"
        names = ("opccomn.idl", "opcda.idl", "opc_ae.idl", "opcSec.idl", "OpcEnum.idl", "opcerror.h", "opcae_er.h", "opcaedef.h")
        self.files = {}
        for name in names:
            data = ("test:" + name).encode()
            (self.source / name).write_bytes(data)
            self.files[name] = ("ignored", hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest())

    def test_commands_and_outputs(self):
        calls = []
        def fake(command, check):
            self.assertTrue(check)
            calls.append(command)
            out = Path(command[command.index("/out") + 1])
            for opt in ("/h", "/iid", "/proxy", "/dlldata"):
                (out / command[command.index(opt) + 1]).write_text("generated")
        with patch.object(mod.staging, "FILES", self.files), patch.object(mod.subprocess, "run", side_effect=fake):
            mod.generate(self.source, self.output, "midl")
        with patch.object(mod.staging, "FILES", self.files):
            mod.verify_generated(self.source, self.output)
            (self.output / "opcda_i.c").write_text("tampered")
            with self.assertRaisesRegex(ValueError, "generated file mismatch"):
                mod.verify_generated(self.source, self.output)
        self.assertEqual(len(calls), 5)
        self.assertEqual((self.output / "opcerror.h").read_bytes(), b"test:opcerror.h")
        self.assertEqual(Path(calls[-1][-1]).name, "OpcEnum.idl")
        self.assertEqual(calls[0][calls[0].index("/I") + 1], str(self.source.resolve()))

    def test_tampering_and_missing_outputs_fail(self):
        (self.source / "opcda.idl").write_bytes(b"modified")
        with patch.object(mod.staging, "FILES", self.files), self.assertRaisesRegex(ValueError, "mismatch"):
            mod.generate(self.source, self.output, "midl")
        self.assertFalse(self.output.exists())
        (self.source / "opcda.idl").write_bytes(b"test:opcda.idl")
        with patch.object(mod.staging, "FILES", self.files), patch.object(mod.subprocess, "run"), self.assertRaisesRegex(ValueError, "MIDL omitted"):
            mod.generate(self.source, self.output, "midl")


if __name__ == "__main__":
    unittest.main()
