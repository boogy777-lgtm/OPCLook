//! Headless tests must neither share state with each other nor touch the real
//! application data: the settings directory is redirected per thread and the
//! PKI store per process (`App::new_test`). A shared `%APPDATA%` / working
//! directory made parallel tests overwrite each other's `settings.json` and
//! race on creating the client key pair — a failure that passed on a retry.
//!
//! This binary contains one test on purpose: it sets `APPDATA` to a sentinel
//! directory before any other thread exists.

use opcda_egui::app::App;
use opcda_egui::i18n::Lang;
use opcda_egui::settings::{Settings, settings_path};
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn entries(dir: &PathBuf) -> Vec<String> {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn headless_tests_use_private_settings_and_pki() {
    let sentinel = std::env::temp_dir().join(format!("opcda-egui-sentinel-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&sentinel);
    std::fs::create_dir_all(&sentinel).expect("sentinel dir");
    // Safe: no other thread of this process exists yet and none is started
    // before this call returns (this binary has a single test).
    unsafe { std::env::set_var("APPDATA", &sentinel) };
    let cwd_pki = std::env::current_dir()
        .map(|d| d.join("opcda-egui").join("pki"))
        .unwrap_or_default();
    let cwd_pki_existed = cwd_pki.exists();

    // Part 1: a test that saves settings and starts a UA search without any
    // isolation of its own leaves the real locations alone.
    let ctx = egui::Context::default();
    let mut app = App::new_test(&ctx);
    app.settings.set_lang(Lang::En);
    app.settings.save();
    app.open_connect_dialog();
    app.search_ua_all();
    let pki = opcda_egui::bff::ua::pki_dir_public();
    assert!(pki.starts_with(std::env::temp_dir()), "{pki:?}");
    assert!(!pki.starts_with(&sentinel), "{pki:?}");
    let settings = settings_path();
    assert!(settings.starts_with(std::env::temp_dir()), "{settings:?}");
    assert!(!settings.starts_with(&sentinel), "{settings:?}");
    assert!(settings.exists(), "the save went to the thread's directory");
    // The search builds the discovery client, which creates the key pair.
    let deadline = Instant::now() + Duration::from_secs(30);
    while !pki.join("own").exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(pki.join("own").exists(), "the private PKI store was used");
    assert_eq!(
        entries(&sentinel),
        Vec::<String>::new(),
        "APPDATA untouched"
    );
    assert_eq!(
        cwd_pki.exists(),
        cwd_pki_existed,
        "no opcda-egui/pki in cwd"
    );

    // Part 2: parallel tests saving different languages never see each other's
    // file (a shared directory would let the last writer win).
    let workers: Vec<_> = (0..8)
        .map(|n| {
            std::thread::spawn(move || {
                let lang = [Lang::Ru, Lang::En, Lang::Zh][n % 3];
                let ctx = egui::Context::default();
                let mut app = App::new_test(&ctx);
                for _ in 0..20 {
                    app.settings.set_lang(lang);
                    app.settings.save();
                    std::thread::sleep(Duration::from_millis(5));
                    assert_eq!(Settings::load().lang(), lang, "worker {n}");
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().expect("worker thread");
    }
    assert_eq!(
        entries(&sentinel),
        Vec::<String>::new(),
        "APPDATA untouched"
    );
    let _ = std::fs::remove_dir_all(&sentinel);
}
