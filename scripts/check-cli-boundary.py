#!/usr/bin/env python3
"""Check the CLI dependency tree and first-party layer edges."""
import json
import pathlib
import re
import subprocess
import sys

if len(sys.argv) != 2:
    raise SystemExit("usage: check-cli-boundary.py TARGET")
command = ["cargo", "tree", "-p", "opclook-cli", "--target", sys.argv[1],
           "--edges", "normal", "--prefix", "none", "--format", "{p}"]
result = subprocess.run(command, text=True, capture_output=True, check=False)
if result.returncode:
    sys.stderr.write(result.stderr)
    raise SystemExit(result.returncode)
names = {line.split()[0] for line in result.stdout.splitlines() if line.split()}
if "opclook-cli" not in names:
    raise SystemExit("CLI package missing from dependency tree")
forbidden = names.intersection({"egui", "eframe", "winit", "rfd", "egui_extras", "egui-winit"})
if forbidden:
    raise SystemExit("CLI has forbidden graphics dependencies: " + ", ".join(sorted(forbidden)))
metadata = subprocess.run(["cargo", "metadata", "--no-deps", "--format-version", "1"], text=True, capture_output=True, check=False)
if metadata.returncode:
    sys.stderr.write(metadata.stderr)
    raise SystemExit(metadata.returncode)
packages = {p["name"]: p for p in json.loads(metadata.stdout)["packages"]}
allowed = {"opclook-contract": set(), "opclook-backend": {"opclook-contract"}, "opclook-engine": {"opclook-contract", "opclook-backend"}, "opclook-locale": {"opclook-contract"}, "opclook-cli": {"opclook-contract", "opclook-engine", "opclook-locale"}, "opclook": {"opclook-contract", "opclook-engine", "opclook-locale"}}
for name, permitted in allowed.items():
    package = packages.get(name)
    if package is None:
        raise SystemExit("missing expected layer: " + name)
    dependencies = {d["name"] for d in package["dependencies"] if d["name"] in allowed and d["kind"] is None}
    if dependencies - permitted:
        raise SystemExit(f"forbidden dependencies for {name}: {sorted(dependencies - permitted)}")
root = pathlib.Path(__file__).resolve().parent.parent
patterns = [
    re.compile(r"\bGateway\s*::\s*(new|default)\s*\("),
    re.compile(r"\b(?:ua|da)::spawn\s*\("),
    re.compile(r"\b(?:gateway|worker)\s*\.\s*(send|try_recv)\s*\("),
    re.compile(r"\b(?:DaRouteManager|RouteService|ResolverPool)\b"),
    re.compile(r"\.\s*(?:prepare_host|prepare_resolved)\s*\("),
]
scanned = []
violations = []
for directory in (root / "src",):
    for path in directory.rglob("*.rs"):
        if path.name == "tests.rs":
            continue
        scanned.append(path)
        for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            if not line.lstrip().startswith("//") and any(pattern.search(line) for pattern in patterns):
                violations.append(f"{path.relative_to(root)}:{number}")
if len(scanned) < 20:
    raise SystemExit("GUI source guard did not find expected files")
if violations:
    raise SystemExit("GUI executes a backend worker directly: " + ", ".join(violations))
print("CLI dependencies and first-party boundaries: OK")
