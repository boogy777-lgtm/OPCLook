fn main() {
    println!("cargo:rerun-if-env-changed=SOFTING_RUNTIME_LIB_DIR");
    if std::env::var_os("CARGO_FEATURE_NATIVE_LIVE").is_none()
        || std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows")
    {
        return;
    }
    let Some(dir) = std::env::var_os("SOFTING_RUNTIME_LIB_DIR") else {
        eprintln!(
            "native-live requires SOFTING_RUNTIME_LIB_DIR pointing at the SDK CMake Release directory"
        );
        std::process::exit(1);
    };
    println!(
        "cargo:rustc-link-search=native={}",
        std::path::PathBuf::from(dir).display()
    );
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
