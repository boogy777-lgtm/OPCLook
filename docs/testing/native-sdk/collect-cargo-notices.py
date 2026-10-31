#!/usr/bin/env python3
"""Collect bounded Cargo package notice evidence; not a license determination."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import tarfile
import tomllib

MAX_FILES_PER_PACKAGE = 64
MAX_ROOT_ENTRIES = 10_000
MAX_MANIFEST_BYTES = 1024 * 1024
MAX_FILE_BYTES = 1024 * 1024
MAX_TOTAL_BYTES = 64 * 1024 * 1024
MAX_ARCHIVE_BYTES = 128 * 1024 * 1024
MAX_ARCHIVE_MEMBERS = 100_000
REGISTRY_SOURCE = "registry+https://github.com/rust-lang/crates.io-index"
UPSTREAM_REVISION = "9ad28fc011002398f2e8a95696a50408a14531d9"
UPSTREAM_LICENSE_BLOB = "a612ad9813b006ce81d1ee438dd784da99a54007"
UPSTREAM_LICENSE_SHA256 = "1f256ecad192880510e84ad60474eab7589218784b9a50bc7ceee34c2b91f1d5"
UPSTREAM_REPOSITORY = "https://github.com/freeopcua/async-opcua"
UPSTREAM_LICENSE_URL = UPSTREAM_REPOSITORY + "/blob/" + UPSTREAM_REVISION + "/LICENSE.md"
UPSTREAM_PACKAGES = frozenset(("async-opcua", "async-opcua-client", "async-opcua-core", "async-opcua-core-namespace", "async-opcua-crypto", "async-opcua-macros", "async-opcua-nodes", "async-opcua-server", "async-opcua-types"))
UPSTREAM_LICENSE = Path(__file__).resolve().parent / "upstream-notices/async-opcua-0.19.0-LICENSE.md"
NAME = re.compile(r"^[A-Za-z0-9_-]+$")
VERSION = re.compile(r"^[A-Za-z0-9.+_-]+$")


def inside(path, root):
    return path == root or root in path.parents


def verify_archive(archive, checksum, prefix, expected):
    """Verify the locked crate archive and exact notice members without extraction."""
    if not archive.is_file():
        return False, set()
    if archive.stat().st_size > MAX_ARCHIVE_BYTES:
        return False, set()
    digest = hashlib.sha256()
    with archive.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    if digest.hexdigest() != checksum:
        raise ValueError(f"registry archive checksum mismatch: {prefix}")
    found = set()
    with tarfile.open(archive, "r:gz") as tar:
        for count, member in enumerate(tar, 1):
            if count > MAX_ARCHIVE_MEMBERS:
                raise ValueError(f"registry archive member limit exceeded: {prefix}")
            if not member.name.startswith(prefix + "/"):
                continue
            relative = member.name[len(prefix) + 1:]
            if relative not in expected:
                continue
            if relative in found:
                raise ValueError(f"duplicate notice member in registry archive: {prefix}/{relative}")
            found.add(relative)
            if not member.isfile() or member.size > MAX_FILE_BYTES:
                raise ValueError(f"invalid notice member in registry archive: {prefix}/{relative}")
            source = tar.extractfile(member)
            if source is None or source.read(MAX_FILE_BYTES + 1) != expected[relative]:
                raise ValueError(f"installed notice differs from locked registry archive: {prefix}/{relative}")
    return True, found


def collect(metadata, rows, workspace, cargo_home, output):
    workspace = workspace.resolve(strict=True)
    if Path(metadata["workspace_root"]).resolve(strict=True) != workspace:
        raise ValueError("Cargo metadata workspace does not match trusted workspace")
    cargo_home = cargo_home.resolve(strict=True)
    registry = (cargo_home / "registry" / "src").resolve(strict=True)
    if not inside(registry, cargo_home):
        raise ValueError("registry source root escapes trusted Cargo home")
    members = set(metadata["workspace_members"])
    lock = tomllib.loads((workspace / "Cargo.lock").read_text(encoding="utf-8"))
    locked = {(p["name"], p["version"], p.get("source")): p for p in lock["package"]}
    records, missing, saved = [], [], {}
    total = 0
    output.mkdir(parents=True, exist_ok=False)
    files_dir = output / "files"
    files_dir.mkdir()
    for row in rows:
        name, version, source = row["name"], row["version"], row.get("source")
        if not NAME.fullmatch(name) or not VERSION.fullmatch(version):
            raise ValueError("invalid package identity")
        entry = {"id": row["id"], "name": name, "version": version, "source": source,
                 "license": row.get("license"), "license_file": row.get("license_file"),
                 "lock_checksum_sha256": None, "archive_verified": None, "files": [], "missing": []}
        records.append(entry)
        lock_entry = locked.get((name, version, source))
        if lock_entry is None:
            raise ValueError(f"package absent from Cargo.lock: {name} {version}")
        if not row.get("license"):
            entry["missing"].append("no declared license expression")
        if row["id"] in members and source is None:
            manifest = Path(row["manifest_path"]).resolve(strict=True)
            if manifest.name != "Cargo.toml" or not inside(manifest, workspace):
                raise ValueError(f"workspace manifest outside trusted root: {name}")
            package_root = manifest.parent
            is_workspace = True
        elif source == REGISTRY_SOURCE:
            checksum = lock_entry.get("checksum")
            if not isinstance(checksum, str) or not re.fullmatch(r"[0-9a-f]{64}", checksum):
                raise ValueError(f"missing registry lock checksum: {name} {version}")
            entry["lock_checksum_sha256"] = checksum
            manifest = Path(row["manifest_path"]).resolve(strict=True)
            package_root = manifest.parent
            if (manifest.name != "Cargo.toml" or package_root.name != f"{name}-{version}"
                    or package_root.parent.parent != registry):
                raise ValueError(f"registry manifest outside trusted package root: {name}")
            if manifest.stat().st_size > MAX_MANIFEST_BYTES:
                raise ValueError(f"installed manifest exceeds size limit: {name}")
            manifest_bytes = manifest.read_bytes()
            installed = tomllib.loads(manifest_bytes.decode("utf-8"))["package"]
            if installed.get("name") != name or installed.get("version") != version:
                raise ValueError(f"installed manifest identity mismatch: {name}")
            is_workspace = False
        else:
            entry["missing"].append("source is not an installed locked registry package or workspace member")
            missing.append({"id": row["id"], "reasons": entry["missing"][:]})
            continue
        candidates = {}
        declared = row.get("license_file")
        if declared:
            declared_path = Path(declared)
            if ".." in declared_path.parts:
                raise ValueError(f"license_file escapes package root: {name}")
            if not declared_path.is_absolute():
                declared_path = package_root / declared_path
            resolved = declared_path.resolve(strict=False)
            if not inside(resolved, package_root):
                raise ValueError(f"license_file escapes package root: {name}")
            if not resolved.is_file():
                entry["missing"].append("declared license_file is absent")
            else:
                candidates[declared_path.relative_to(package_root).as_posix()] = resolved
        names = []
        for index, path in enumerate(package_root.iterdir(), 1):
            if index > MAX_ROOT_ENTRIES:
                entry["missing"].append("package-root scan exceeded entry limit")
                names = []
                break
            if path.name.upper().startswith(("LICENSE", "COPYING", "NOTICE")):
                names.append(path)
                if len(names) > MAX_FILES_PER_PACKAGE:
                    entry["missing"].append("too many package-root license/notice candidates")
                    names = []
                    break
        names.sort(key=lambda p: p.name)
        folded = set()
        for path in names:
            folded_name = path.name.casefold()
            if folded_name in folded:
                raise ValueError(f"ambiguous case-insensitive notice name: {name}")
            folded.add(folded_name)
            resolved = path.resolve(strict=True)
            if not inside(resolved, package_root):
                raise ValueError(f"notice symlink escapes package root: {name}")
            if resolved.is_file():
                candidates[path.name] = resolved
        if not candidates and is_workspace:
            fallback = (workspace / "LICENSE").resolve(strict=True)
            if not inside(fallback, workspace) or not fallback.is_file():
                raise ValueError("invalid workspace LICENSE")
            candidates["<workspace>/LICENSE"] = fallback
        source_bytes = {}
        for relative, path in sorted(candidates.items()):
            size = path.stat().st_size
            if size > MAX_FILE_BYTES:
                entry["missing"].append(f"notice exceeds per-file limit: {relative}")
                continue
            data = path.read_bytes()
            if len(data) != size:
                raise ValueError(f"notice changed while reading: {name} {relative}")
            source_bytes[relative] = data
        verified, members_found = (None, set())
        if not is_workspace:
            archive = cargo_home / "registry" / "cache" / package_root.parent.name / f"{name}-{version}.crate"
            cache_root = (cargo_home / "registry" / "cache").resolve(strict=False)
            archive = archive.resolve(strict=False)
            if not inside(cache_root, cargo_home) or not inside(archive, cache_root):
                raise ValueError(f"registry archive escapes trusted cache: {name}")
            # Only these exact locked crates may use the separately pinned root
            # license. Verify the package's own VCS receipt and normalized
            # manifest as members of its checksum-verified archive first.
            curated = (not source_bytes and not declared and not entry["missing"]
                       and name in UPSTREAM_PACKAGES and version == "0.19.0"
                       and row.get("license") == "MPL-2.0"
                       and installed.get("license") == "MPL-2.0"
                       and installed.get("repository") == UPSTREAM_REPOSITORY)
            expected = dict(source_bytes)
            if curated:
                vcs_path = (package_root / ".cargo_vcs_info.json").resolve(strict=False)
                if not inside(vcs_path, package_root):
                    raise ValueError(f"VCS receipt escapes package root: {name}")
                if vcs_path.is_file() and vcs_path.stat().st_size <= MAX_MANIFEST_BYTES:
                    vcs_bytes = vcs_path.read_bytes()
                    try:
                        vcs = json.loads(vcs_bytes.decode("utf-8"))
                    except (UnicodeDecodeError, json.JSONDecodeError):
                        vcs = {}
                    if (isinstance(vcs, dict) and isinstance(vcs.get("git"), dict)
                            and vcs["git"].get("sha1") == UPSTREAM_REVISION and vcs.get("path_in_vcs") == name):
                        expected[".cargo_vcs_info.json"] = vcs_bytes
                        expected["Cargo.toml"] = manifest_bytes
                    else:
                        curated = False
                else:
                    curated = False
            verified, members_found = verify_archive(archive, checksum, f"{name}-{version}", expected)
            if curated and verified and {"Cargo.toml", ".cargo_vcs_info.json"}.issubset(members_found):
                notice_file = UPSTREAM_LICENSE.resolve(strict=True)
                if not inside(notice_file, Path(__file__).resolve().parent):
                    raise ValueError("curated upstream license escapes trusted source root")
                if notice_file.stat().st_size > MAX_FILE_BYTES:
                    raise ValueError("curated upstream license exceeds size limit")
                license_bytes = notice_file.read_bytes().replace(b"\r\n", b"\n")
                blob = hashlib.sha1(b"blob " + str(len(license_bytes)).encode("ascii") + b"\0" + license_bytes).hexdigest()
                if hashlib.sha256(license_bytes).hexdigest() != UPSTREAM_LICENSE_SHA256 or blob != UPSTREAM_LICENSE_BLOB:
                    raise ValueError("curated upstream license does not match pinned source")
                source_bytes["<upstream>/LICENSE.md"] = license_bytes
                entry["upstream_license"] = {"repository": UPSTREAM_REPOSITORY, "revision": UPSTREAM_REVISION,
                    "git_blob": UPSTREAM_LICENSE_BLOB, "sha256": UPSTREAM_LICENSE_SHA256,
                    "url": UPSTREAM_LICENSE_URL, "vcs_and_manifest_archive_verified": True}
            entry["archive_verified"] = verified
            if not verified:
                entry["missing"].append("locked registry archive unavailable or exceeds limit")
            for relative in sorted(source_bytes.keys() - members_found):
                if verified and relative != "<upstream>/LICENSE.md":
                    entry["missing"].append(f"notice not found in locked registry archive: {relative}")
        for relative, data in sorted(source_bytes.items()):
            digest = hashlib.sha256(data).hexdigest()
            if digest not in saved:
                total += len(data)
                if total > MAX_TOTAL_BYTES:
                    raise ValueError("Cargo notice collection exceeds total byte limit")
                (files_dir / digest).write_bytes(data)
                saved[digest] = len(data)
            upstream = relative == "<upstream>/LICENSE.md"
            entry["files"].append({"path": relative, "sha256": digest, "bytes": len(data),
                                   "source_kind": "verified-upstream-root-license" if upstream else ("workspace" if is_workspace else "registry-package"),
                                   "archive_member_verified": None if is_workspace or upstream else relative in members_found,
                                   "bundle_path": "notices/cargo/files/" + digest})
        if not entry["files"]:
            entry["missing"].append("no package license/notice text found")
        if entry["missing"]:
            missing.append({"id": row["id"], "reasons": entry["missing"][:]})
    result = {"schema": 1, "scope": "resolved Cargo normal/build superset, not a PE linker set or legal clearance",
              "packages": records, "missing": missing, "unique_files": len(saved), "unique_bytes": total}
    (output / "manifest.json").write_text(json.dumps(result, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--metadata", required=True, type=Path)
    parser.add_argument("--workspace", required=True, type=Path)
    parser.add_argument("--cargo-home", type=Path, default=Path(os.environ.get("CARGO_HOME", str(Path.home() / ".cargo"))))
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    try:
        metadata = json.loads(args.metadata.read_text(encoding="utf-8"))
        spec = importlib.util.spec_from_file_location("opclook_preview", Path(__file__).with_name("package-preview.py"))
        preview = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(preview)
        result = collect(metadata, preview.normal_build_licenses(metadata), args.workspace, args.cargo_home, args.output)
    except (OSError, ValueError, KeyError, tomllib.TOMLDecodeError, json.JSONDecodeError, tarfile.TarError) as error:
        parser.exit(1, f"Cargo notice inventory FAILED: {error}\n")
    print(f"Cargo notice inventory: {len(result['packages'])} packages, {result['unique_files']} unique texts, {len(result['missing'])} packages with missing evidence; NOT legal clearance")


if __name__ == "__main__":
    main()
