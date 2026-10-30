# N1.LICENSE: pinned DA source/build inventory — OPEN

This is an engineering inventory, not a legal opinion or permission to use or
distribute any component. It covers the current local Win32 x86 native-DA
preview and test fixture, not the rest of OPCLook or an N4 release. The
candidate remains opt-in; production DA is still legacy.

## Evidence and unresolved rights

* Softing [SDK root MIT notice](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/LICENSE), Git blob `8e099ed5018feb4a67fb8e1d8f2ef397c4fcc240`, is reproduced in [THIRD-PARTY-NOTICES.txt](THIRD-PARTY-NOTICES.txt). Pinned commit is `5130e763998189c822882434c43e6be2eb0174cf`. Its complete recursive Git tree returned 3,940 entries, `truncated=false`; the manifest selects 271 blobs in candidate and fixture source/include regions, including a deliberately broad header/demo superset. Git blob IDs in `license-sdk-tree.json` bind their contents to this tree, not to a moving release.
* The source directly includes separately attributed OPC Foundation material.
  [OPCDA.IDL](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/core/src/opc/OPCDA.IDL) (`fe590c3ac946944ffc71914310b1aeca58983ace`) and [OPCCOMN.IDL](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/core/src/opc/OPCCOMN.IDL) (`2a1cffe53f549e7fb409e733e72dd639ce7a27b5`) state OPC Foundation copyright, all rights reserved, and refer to the License Grant in the relevant specification. [OpcEnum.idl](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/core/src/opc/OpcEnum.idl) and [opcSec.idl](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/core/src/opc/opcSec.idl), plus [opc_ae.idl](https://github.com/SoftingIndustrial/OPC-Classic-SDK/blob/5130e763998189c822882434c43e6be2eb0174cf/development/core/src/opc/opc_ae.idl), contain similar statements. Their generated headers/IID sources participate in the build; the test proxy is generated from the first two IDLs. An attribution is added to notices, not an assertion of grant.
* The [OPC Foundation Classic Data Access page](https://opcfoundation.org/developer-tools/specifications-classic/data-access/) describes the specifications and sample interface code, identifies the Specifications Agreement and paid-member access; the published [agreement](https://opcfoundation.org/license/specifications/1.15/) describes conditions for authorized licensees. We did not download a specification or accept terms, and did not establish that this current agreement is the exact grant referenced by the older IDL headers.
* The Foundation's [Core Components page](https://opcfoundation.org/developer-tools/samples-and-tools-classic/core-components/) says its current package is under OPC MIT. Its official [Core Components repository at `efe0d1d1ea86a8a727bf26a501a261765e836766`](https://github.com/OPCF-Members/OPC-Classic-CoreComponents/tree/efe0d1d1ea86a8a727bf26a501a261765e836766) contains MIT-labelled corresponding IDLs. A comparison of full files from the first `import` onward, normalizing CRLF to LF, stripping trailing spaces per line and leading/trailing whitespace, produced equal bodies for DA, Common, OpcEnum, Security and A&E; headers and Git blobs differ. The official repo README also describes the archive and its EOL status. This is relevant primary evidence, not an automatic determination of the grant for the exact older Softing copies or their generated files.

  | Softing source | Official MIT-labelled corresponding IDL Git blob |
  | --- | --- |
  | `OPCDA.IDL` | [`Source/DataAccess/ProxyStub/opcda.idl`](https://github.com/OPCF-Members/OPC-Classic-CoreComponents/blob/efe0d1d1ea86a8a727bf26a501a261765e836766/Source/DataAccess/ProxyStub/opcda.idl) `d7e14cdc91c2a008bb2d365229880584482db0df` |
  | `OPCCOMN.IDL` | [`Source/Common/ProxyStub/opccomn.idl`](https://github.com/OPCF-Members/OPC-Classic-CoreComponents/blob/efe0d1d1ea86a8a727bf26a501a261765e836766/Source/Common/ProxyStub/opccomn.idl) `b18e7ca32f1e8acd0be6ab8b48745f48367199e0` |
  | `OpcEnum.idl` | [`Source/Common/ServerEnumerator/OpcEnum.idl`](https://github.com/OPCF-Members/OPC-Classic-CoreComponents/blob/efe0d1d1ea86a8a727bf26a501a261765e836766/Source/Common/ServerEnumerator/OpcEnum.idl) `851bb2acc857ea4f669c176dde523ad93f2e9a83` |
  | `opcSec.idl` | [`Source/Security/ProxyStub/opcSec.idl`](https://github.com/OPCF-Members/OPC-Classic-CoreComponents/blob/efe0d1d1ea86a8a727bf26a501a261765e836766/Source/Security/ProxyStub/opcSec.idl) `5d63c06127d857fb9b7910d4e560afe1e0299c5c` |
  | `opc_ae.idl` | [`Source/AlarmsAndEvents/ProxyStub/opc_ae.idl`](https://github.com/OPCF-Members/OPC-Classic-CoreComponents/blob/efe0d1d1ea86a8a727bf26a501a261765e836766/Source/AlarmsAndEvents/ProxyStub/opc_ae.idl) `3b81175435fe200da5862be8cc2340afbabf83ef` |

* **Open N4 gate:** applicable grant for the exact SDK copies and their generated sources has not been established. Obtain the historical grants referenced by those headers or authoritative confirmation of applicability of the current MIT-labelled versions, review the source/generated/object/distribution scope for each actual artifact, and determine required notices. Do not infer incompatibility or a permission requirement from the headers alone. No terms are accepted here.
* The SDK's `SOCmn/expat/xmlparse.c` identifies Expat and refers to COPYING. Existing notices reproduce the [upstream Expat 2.0.0 MIT text](https://github.com/libexpat/libexpat/blob/1a6d93c530e1aa1ca3a567b05bad894e19027d18/expat/COPYING); the pinned SDK's Expat inputs are listed with blob IDs in `softing-expat.gitsha`. The tree manifest binds the SDK copy; it does not independently establish that every byte matches the upstream release.

## Build and distribution boundaries

| Scope | Inputs and configuration | Evidence / limit |
| --- | --- | --- |
| Native DA candidate | `core-probe/CMakeLists.txt`: globs `SOCmn/*.cpp`, `SOClt/*.cpp`, `SODaC/*.cpp`, `opc/*_i.c`; excludes `SOCmnExpat.cpp` and builds `xmlrole.c`, `xmltok.c`, `xmlparse.c` separately as `softing_expat`. `softing_da_bridge` adds OPCLook `adapter.cpp`, `runtime.cpp`. `core-probe/stdafx.h` enables DCOM, SOCLT, SODAC, static/no forced SDK libs; disables SOAP default, Watch and XML methods, but common XML remains enabled. | Candidate static libraries `softing_da_bridge` and `softing_expat` are linked by `crates/backend/build.rs` for Windows + `native-da-preview`. CMake glob is the source-list evidence, not a measured final linker map. Generated OPC IID C files are included even if a feature is unused. |
| SDK changes | `softing-status-alignment.patch` and `softing-com-balance.patch`, applied by `softing_patch.py` to exact original Git blobs before the bridge build. | Patches modify SDK source; retain applicable upstream notices. They do not supply an additional grant for OPC Foundation files. |
| Local test only | `server-fixture/CMakeLists.txt` additionally compiles `SOSrv`, `SODaS`, `SOSAllocHelpers.cpp`, `SOProxy_i.c`; independent test client and proxy. MIDL generates OPCDA/OPCCOMN `_p.c`, `_i.c` and headers from pinned IDL. | Fixture server/client/proxy are test artifacts, not shown to ship in the product. If their distribution changes, reassess separately. The generated RPC code does not establish independent rights in the input IDL. |
| Platform and tools | Candidate links `ole32`, `oleaut32`, `uuid`, `advapi32`, `ws2_32`, `mpr`, `rpcrt4`; Rust build also lists `user32`, `gdi32`, `shell32`. Fixture proxy links `rpcrt4`, `oleaut32`, `uuid`. MSVC/Windows SDK headers, ATL, MIDL, system IDL imports, CMake, Python, Rust/Cargo are platform/build dependencies. | System import libraries and tool installations are not an assertion of redistributable runtime DLLs. Exact Windows SDK/MSVC/runtime and release packaging/redistribution terms must be recorded for the actual release build; `windows-latest` is not a toolchain pin. |

OTB, TBC and private activation components are not in these target source or
link lists; the C++ facade has a separate header-only feasibility probe. This
is not proof that the entire SDK tree has no other licenses or dependencies.
Rust dependency/Cargo.lock license and release packaging audits are outside
this SDK-specific inventory and remain required separately. Existing notices
live under `docs/testing/native-sdk`; the actual installer/artifact must be
checked to include all applicable notices before distribution.

## Mechanical change guard

`license-sdk-tree.json` records the exact Git blobs in selected SDK directories
and SHA-256 of the CMake, feature headers, build.rs and patch configuration.
`verify-license-closure.py` verifies a **clean, unpatched Git checkout** at the
pin, compares the selected tree, rejects tracked modifications and all extra
files (including ignored files) in selected regions, and checks profile hashes.
Run before applying the SDK patches:

```sh
python docs/testing/native-sdk/verify-license-closure.py /path/to/softing-sdk
python docs/testing/native-sdk/test-license-closure.py
```

The script prints `legal clearance NOT established` on success. It deliberately
fails when the reviewed profile changes; review changes, regenerate the
manifest from the immutable upstream tree, and update this inventory. A proposed
CI step immediately after SDK checkout, before configure/patch, is:

```yaml
- name: Verify reviewed native SDK inventory
  shell: pwsh
  run: |
    python opclook/docs/testing/native-sdk/verify-license-closure.py softing-sdk
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    python opclook/docs/testing/native-sdk/test-license-closure.py
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
```

The guard does not parse all compiler includes, resolve every conditional,
verify toolchain/system binaries, audit Cargo dependencies, prove notices are
shipped, or determine rights. Changes to OPCLook C++ sources, generated build
rules, environment, external SDKs or workflows can add dependencies without
changing a guarded profile file. A full Windows configured dependency/include
trace, linker inputs/map and released-artifact inventory remain pending. Local
checker tests use temporary Git repositories, not a real Windows SDK build.
Do not mark N1.LICENSE or N4 closed until these facts and the OPC grant are
resolved and the intended release's notices are verified.
