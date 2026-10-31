# N1: private preview Cargo notice inventory

This is an engineering inventory, not a legal conclusion or release approval.
The packager follows the Cargo metadata graph from `opclook-cli` through normal
and build dependencies for the requested platform and active preview feature.
It is a resolved superset, not proof that every package is in the linked PE;
the retained link map is separate evidence. Dev dependencies are excluded
unless also reachable by a normal/build edge.

For each reachable locked package, the collector records its package ID,
version, source, declared license expression, Cargo.lock registry SHA-256, and
an explicit list of missing evidence. It copies the actual bytes of declared
`license_file` and package-root `LICENSE*`, `COPYING*`, and `NOTICE*` files
into content-addressed `notices/cargo/files/`, with per-file SHA-256 and length.
Duplicate bytes are stored once. A workspace member with no local notice uses
only the trusted OPCLook workspace `LICENSE`. Registry package manifests must
belong to the matching package under the trusted Cargo home registry source;
other path/git sources are recorded as missing, without scanning their parents.
Declared paths and symlinks escaping the package root are rejected. Case-folded
ambiguous names, mismatched lock checksums and modified installed notice bytes
are errors. There are limits of 64 root candidates per package, 1 MiB per
notice, 64 MiB total unique notice bytes, 128 MiB per compressed crate archive,
and 100,000 members per archive. Missing or over-limit inputs are explicit;
they do not become a green license finding.

For registry packages the `.crate` archive is SHA-256 checked against Cargo.lock,
then each selected package text is matched byte-for-byte against its regular-file member
without extracting archive paths. Missing archives or absent members are in the
missing list. This verifies selected notice bytes, not the entire installed
source tree, package provenance beyond Cargo's pinned checksum, or any legal
obligation. OPCLook workspace text is copied from the checked-out tree; package
receipt includes the OPCLook commit. The preview bundle retains the metadata,
lockfile, scripts, and the complete file hash manifest.

## Measured local inventory

At OPCLook `42594678e949370da7c94e4cdde18f671041cc41`, with Cargo.lock SHA-256
`fd2c1d005d550d35a98158d9e215eac663fcd23417711446e4262734a44eea89`, the locked
`i686-pc-windows-msvc` filtered metadata with `native-da-preview` produced
183 reachable packages: 178 crates.io packages and 5 workspace members. All
178 locally cached `.crate` archives matched their lock checksums. There were
94 unique notice texts, totalling 295,854 bytes; the missing list was empty.
Nine version 0.19.0 `async-opcua` crates carry no notice text in their own
archives. Their locked manifests and `.cargo_vcs_info.json` archive members
match the exact repository and revision `9ad28fc011002398f2e8a95696a50408a14531d9`;
the separately retained upstream root LICENSE has git blob
`a612ad9813b006ce81d1ee438dd784da99a54007` and SHA-256
`1f256ecad192880510e84ad60474eab7589218784b9a50bc7ceee34c2b91f1d5`.
This narrowly whitelisted fallback is recorded as `verified-upstream-root-license`,
not as a license file found in the crate archive. The Git source bytes are
reconstructed from CRLF checkouts before both hashes are checked. Other missing
texts remain explicit; these records do not determine legal obligations. Counts are for this local
metadata and cache; Windows CI must regenerate them from its own build inputs.

```sh
cargo metadata --locked --format-version 1 --filter-platform i686-pc-windows-msvc --features opclook-backend/native-da-preview > preview-metadata.json
python docs/testing/native-sdk/collect-cargo-notices.py --metadata preview-metadata.json --workspace . --output ../cargo-notice-inventory
python docs/testing/native-sdk/test-collect-cargo-notices.py
python docs/testing/native-sdk/test-package-preview.py
```

`package-preview.py` performs the same collection during bundle creation. Its
existing native receipt verifies Softing SDK `5130e763998189c822882434c43e6be2eb0174cf`,
the 271-entry source inventory and reviewed patches, and official OPCF inputs
`efe0d1d1ea86a8a727bf26a501a261765e836766`. The actual SDK `LICENSE` is now
copied in addition to the existing `THIRD-PARTY-NOTICES.txt` (SHA-256
`951572b41881d215899476822eacd4ba612e0f13e264fc840dc5a97fa50def49`), which
contains the Softing, OPCF and Expat notice material. The staged OPCF input
files and generated receipt remain in the bundle. The server fixture is
test-only; system Windows components, MSVC/CRT and toolchains are separate from
this Cargo traversal. Their redistribution and other applicable release checks
remain manual open gates. No acceptance of third-party terms is performed.
