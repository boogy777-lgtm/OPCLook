fn main() {
    println!("cargo:rerun-if-env-changed=OPCLOOK_HELPER_TEST_NO_MANIFEST");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=nethelper.manifest");
        if std::env::var_os("OPCLOOK_HELPER_TEST_NO_MANIFEST").is_some() {
            println!(
                "cargo:warning=Helper elevation manifest disabled for unit tests only; do not ship this build"
            );
            return;
        }
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTUAC:level='requireAdministrator' uiAccess='false'");
    }
}
