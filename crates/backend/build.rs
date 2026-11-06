fn main() {
    println!("cargo:rerun-if-env-changed=SOFTING_RUNTIME_LIB_DIR");
    let override_dir = std::env::var_os("SOFTING_RUNTIME_LIB_DIR");
    if std::env::var_os("CARGO_FEATURE_NATIVE_DA_TEST_HOOKS").is_some()
        && override_dir.as_ref().is_none_or(|dir| dir.is_empty())
    {
        eprintln!(
            "native-da-test-hooks requires SOFTING_RUNTIME_LIB_DIR pointing to a hook-enabled x86 bridge; vendored production libraries are built without hooks"
        );
        std::process::exit(1);
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let dir = override_dir
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../native/softing-da/lib/i686-pc-windows-msvc")
        });
    println!("cargo:rerun-if-changed={}", dir.display());
    for file in ["softing_da_bridge.lib", "softing_expat.lib"] {
        if !dir.join(file).is_file() {
            eprintln!("Missing Softing library: {}", dir.join(file).display());
            std::process::exit(1);
        }
    }
    println!("cargo:rustc-link-search=native={}", dir.display());
    for lib in [
        "softing_da_bridge",
        "softing_expat",
        "ole32",
        "oleaut32",
        "uuid",
        "advapi32",
        "ws2_32",
        "mpr",
        "rpcrt4",
        "user32",
        "gdi32",
        "shell32",
    ] {
        println!("cargo:rustc-link-lib={lib}");
    }
}
