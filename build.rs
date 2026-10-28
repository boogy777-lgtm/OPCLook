fn main() {
    println!("cargo:rerun-if-env-changed=OPCLOOK_CHECK_ONLY_SKIP_RESOURCES");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        if std::env::var_os("OPCLOOK_CHECK_ONLY_SKIP_RESOURCES").is_some() {
            println!(
                "cargo:warning=Windows resources skipped for cross-check only; do not ship this build"
            );
            return;
        }
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.compile().unwrap();
    }
}
