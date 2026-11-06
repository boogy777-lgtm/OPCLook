# Vendored x86 Softing bridge

Built locally on 2026-11-05, Release Win32, Softing SDK commit
`5130e763998189c822882434c43e6be2eb0174cf` with the two reviewed patches
`softing-status-alignment.patch` and `softing-com-balance.patch`, and official
OPCF inputs at `efe0d1d1ea86a8a727bf26a501a261765e836766`. CMake profile:
`OPCL_BUILD_PROBES=OFF`, `OPCL_USE_LEGACY_OPC=OFF`. MSVC 19.51.36252.0,
Windows SDK 10.0.26100.0. SHA-256 values are in `SHA256SUMS`. Hashes bind
these specific archives; byte-reproducibility across builds is not claimed.
`dumpbin /linkermember:1 softing_da_bridge.lib` found no `opcl_da_test_`
symbols in this build; it does not replace runtime interop testing. The archive
was rebuilt after the OPCLook discovery stale-entry fix: unresolvable ProgID
entries in the local OPC registry are skipped and reported through flag bit 1
instead of aborting the whole server list.

From an x86 Visual Studio developer prompt at the repository root, prepare
clean pinned SDK and OPCF checkouts, then:

```cmd
python docs\testing\native-sdk\verify-license-closure.py C:\path\softing-sdk
python docs\testing\native-sdk\softing_patch.py C:\path\softing-sdk
python docs\testing\native-sdk\stage-opcf-mit-inputs.py --repo C:\path\opc-core-components --output C:\path\opcf-staged
python docs\testing\native-sdk\generate-opcf-midl.py --source C:\path\opcf-staged --output C:\path\opcf-generated
cmake -S docs\testing\native-sdk\core-probe -B C:\path\bridge-build -A Win32 -DSOFTING_SDK_ROOT=C:/path/softing-sdk -DOPCF_MIT_STAGED=C:/path/opcf-staged -DOPCF_MIT_GENERATED=C:/path/opcf-generated -DOPCL_BUILD_PROBES=OFF -DOPCL_USE_LEGACY_OPC=OFF
cmake --build C:\path\bridge-build --config Release --target softing_da_bridge --parallel 2
```

Revalidate provenance and notice requirements before distributing. Owner closure
of N1.LICENSE is not a legal analysis; see `docs/testing/native-sdk/license-closure.md`.
