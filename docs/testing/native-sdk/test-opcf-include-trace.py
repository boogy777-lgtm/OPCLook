#!/usr/bin/env python3
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("trace", Path(__file__).with_name("check-opcf-include-trace.py"))
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)


class IncludeTraceTests(unittest.TestCase):
    def test_complete_localized_and_casefolded(self):
        log = "Hinweis: C:\\Build\\GEN\\OPCDA.H\nNote: C:\\build\\gen\\opccomn.h\n"
        self.assertEqual(mod.check(log, "C:/sdk/development/core/src/opc", "C:/build/gen", ["opcda.h", "opccomn.h"]), 2)

    def test_missing_and_old_even_if_good_also_seen(self):
        with self.assertRaisesRegex(ValueError, "absent"):
            mod.check("", "/sdk/opc", "/build/gen", ["opcda.h"])
        with self.assertRaisesRegex(ValueError, "old SDK"):
            mod.check("/sdk/opc/opcda.h\n/build/gen/opcda.h", "/sdk/opc", "/build/gen", ["opcda.h"])

    def test_prefix_or_bad_roots_do_not_pass(self):
        with self.assertRaisesRegex(ValueError, "absent"):
            mod.check("/build/gen/opcda.h.old", "/sdk/opc", "/build/gen", ["opcda.h"])
        with self.assertRaisesRegex(ValueError, "invalid trace roots"):
            mod.check("", "/x", "/x", ["opcda.h"])


if __name__ == "__main__":
    unittest.main()
