//! Session-scoped trust of a server certificate («Доверять сертификату»).
//!
//! The OPC UA client library only accepts a server certificate that sits in the
//! PKI `trusted` folder, and its own "trust unknown certificates" switch writes
//! the certificate there PERMANENTLY. A connect dialog checkbox must not do
//! that: ticking it means "I trust this server for this session".
//!
//! [`TrustLease`] implements exactly that. While a session that asked for trust
//! is alive, the server certificate is placed in `trusted`; when the last such
//! session ends the file is removed again. A certificate the user has put into
//! `trusted` themselves (the explicit, permanent action) is never touched. The
//! leased files are listed in `session-trusted.list` so that a crash cannot make
//! them permanent: the list is purged the next time the application connects.
//!
//! What the "trust" choice still means for validation: the certificate is no
//! longer required to be pre-imported, and its validity period, host name and
//! application URI are not checked (an industrial controller's certificate often
//! fails those). The key length against the negotiated policy and the identity
//! of the certificate are always checked by the library.

use opcua::crypto::{CertificateStore, X509};
use opcua::types::ByteString;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, Once, PoisonError};

const TRUSTED_DIR: &str = "trusted";
const REJECTED_DIR: &str = "rejected";
const MARKER_FILE: &str = "session-trusted.list";

/// Leased certificate file -> number of live sessions using it.
static LEASES: LazyLock<Mutex<HashMap<PathBuf, usize>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static PURGE: Once = Once::new();

/// Keeps a server certificate trusted for as long as it lives.
pub(super) struct TrustLease {
    trusted_file: PathBuf,
    marker: PathBuf,
}

impl Drop for TrustLease {
    fn drop(&mut self) {
        let mut leases = LEASES.lock().unwrap_or_else(PoisonError::into_inner);
        let last = match leases.get_mut(&self.trusted_file) {
            Some(users) if *users > 1 => {
                *users -= 1;
                false
            }
            Some(_) => {
                leases.remove(&self.trusted_file);
                true
            }
            None => false,
        };
        if last {
            let _ = std::fs::remove_file(&self.trusted_file);
            forget_in_marker(&self.marker, &self.trusted_file);
        }
    }
}

/// Trusts `server_certificate` for the lifetime of the returned lease.
///
/// `Ok(None)` means nothing is leased: there is no certificate (an unsecured
/// endpoint) or the user already trusts it permanently.
pub(super) fn trust_for_session(
    pki: &Path,
    server_certificate: &ByteString,
) -> Result<Option<TrustLease>, String> {
    let Some(der) = server_certificate
        .value
        .as_deref()
        .filter(|d| !d.is_empty())
    else {
        return Ok(None);
    };
    let cert = X509::from_byte_string(server_certificate)
        .map_err(|e| format!("не удалось разобрать сертификат сервера: {e}"))?;
    let name = CertificateStore::cert_file_name(&cert);
    let trusted_dir = pki.join(TRUSTED_DIR);
    let trusted_file = trusted_dir.join(&name);
    let marker = pki.join(MARKER_FILE);
    purge_stale(pki);

    let mut leases = LEASES.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(users) = leases.get_mut(&trusted_file) {
        *users += 1;
        return Ok(Some(TrustLease {
            trusted_file,
            marker,
        }));
    }
    if trusted_file.exists() {
        // Trusted by an explicit user action: permanent, not ours to remove.
        return Ok(None);
    }
    std::fs::create_dir_all(&trusted_dir)
        .map_err(|e| format!("не удалось создать {}: {e}", trusted_dir.display()))?;
    // Record the file BEFORE creating it so a crash in between leaves no
    // permanent trust behind.
    remember_in_marker(&marker, &trusted_file)?;
    std::fs::write(&trusted_file, der)
        .map_err(|e| format!("не удалось сохранить {}: {e}", trusted_file.display()))?;
    // An earlier refused attempt parks the certificate in `rejected`, and the
    // library rejects anything found there even when it is trusted.
    let _ = std::fs::remove_file(pki.join(REJECTED_DIR).join(&name));
    leases.insert(trusted_file.clone(), 1);
    Ok(Some(TrustLease {
        trusted_file,
        marker,
    }))
}

/// Untrusts certificates a previous run leased and never released. Done once per
/// process, on the first connect (with or without the trust choice), so a crash
/// cannot leave a server trusted for good.
pub(super) fn purge_stale(pki: &Path) {
    PURGE.call_once(|| purge_leftovers(pki));
}

fn remember_in_marker(marker: &Path, file: &Path) -> Result<(), String> {
    let mut lines = read_marker(marker);
    let text = file.to_string_lossy().to_string();
    if !lines.contains(&text) {
        lines.push(text);
    }
    write_marker(marker, &lines)
}

fn forget_in_marker(marker: &Path, file: &Path) {
    let text = file.to_string_lossy().to_string();
    let lines: Vec<String> = read_marker(marker)
        .into_iter()
        .filter(|line| *line != text)
        .collect();
    let _ = write_marker(marker, &lines);
}

fn read_marker(marker: &Path) -> Vec<String> {
    std::fs::read_to_string(marker)
        .map(|text| {
            text.lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn write_marker(marker: &Path, lines: &[String]) -> Result<(), String> {
    if lines.is_empty() {
        let _ = std::fs::remove_file(marker);
        return Ok(());
    }
    std::fs::write(marker, lines.join("\n"))
        .map_err(|e| format!("не удалось записать {}: {e}", marker.display()))
}

/// Removes certificates a previous run leased and never got to release (it was
/// killed). Only files inside the `trusted` folder are ever deleted.
fn purge_leftovers(pki: &Path) {
    let marker = pki.join(MARKER_FILE);
    let trusted_dir = pki.join(TRUSTED_DIR);
    for line in read_marker(&marker) {
        let file = PathBuf::from(&line);
        if file.parent() == Some(trusted_dir.as_path()) {
            let _ = std::fs::remove_file(&file);
        }
    }
    let _ = std::fs::remove_file(&marker);
}

#[cfg(test)]
mod tests {
    use super::*;
    use opcua::crypto::{X509, X509Data};

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("opclook-trust-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    fn certificate(name: &str) -> (ByteString, String) {
        let data = X509Data {
            key_size: 2048,
            common_name: name.to_string(),
            organization: "opclook".to_string(),
            organizational_unit: "tests".to_string(),
            country: "RU".to_string(),
            state: "".to_string(),
            alt_host_names: vec![format!("urn:{name}")].into(),
            certificate_duration_days: 1,
        };
        let (cert, _) = X509::cert_and_pkey(&data).expect("certificate");
        let file = CertificateStore::cert_file_name(&cert);
        (cert.as_byte_string(), file)
    }

    #[test]
    fn trust_lasts_exactly_as_long_as_the_session() {
        let pki = scratch("lease");
        let (cert, file) = certificate("lease-server");
        let trusted_file = pki.join("trusted").join(&file);

        let lease = trust_for_session(&pki, &cert).unwrap().expect("leased");
        assert!(trusted_file.exists(), "trusted while the session lives");
        drop(lease);
        assert!(
            !trusted_file.exists(),
            "the certificate must not stay trusted after the session"
        );
        assert!(!pki.join(MARKER_FILE).exists());
        let _ = std::fs::remove_dir_all(&pki);
    }

    #[test]
    fn concurrent_sessions_share_one_lease() {
        let pki = scratch("shared");
        let (cert, file) = certificate("shared-server");
        let trusted_file = pki.join("trusted").join(&file);

        let first = trust_for_session(&pki, &cert).unwrap().expect("first");
        let second = trust_for_session(&pki, &cert).unwrap().expect("second");
        drop(first);
        assert!(
            trusted_file.exists(),
            "the second session still needs the trust"
        );
        drop(second);
        assert!(!trusted_file.exists());
        let _ = std::fs::remove_dir_all(&pki);
    }

    #[test]
    fn a_certificate_the_user_trusted_permanently_is_never_removed() {
        let pki = scratch("permanent");
        let (cert, file) = certificate("permanent-server");
        let trusted_file = pki.join("trusted").join(&file);
        std::fs::create_dir_all(pki.join("trusted")).unwrap();
        std::fs::write(&trusted_file, b"imported by the user").unwrap();

        let lease = trust_for_session(&pki, &cert).unwrap();
        assert!(lease.is_none(), "nothing to lease: already trusted");
        assert_eq!(
            std::fs::read(&trusted_file).unwrap(),
            b"imported by the user"
        );
        let _ = std::fs::remove_dir_all(&pki);
    }

    #[test]
    fn a_previously_rejected_certificate_is_released_by_an_explicit_trust() {
        let pki = scratch("rejected");
        let (cert, file) = certificate("rejected-server");
        let rejected_file = pki.join("rejected").join(&file);
        std::fs::create_dir_all(pki.join("rejected")).unwrap();
        std::fs::write(&rejected_file, b"parked").unwrap();

        let lease = trust_for_session(&pki, &cert).unwrap().expect("leased");
        assert!(
            !rejected_file.exists(),
            "rejected copy would veto the trust"
        );
        drop(lease);
        let _ = std::fs::remove_dir_all(&pki);
    }

    #[test]
    fn leftovers_of_a_crashed_run_are_purged_and_only_inside_trusted() {
        let pki = scratch("purge");
        std::fs::create_dir_all(pki.join("trusted")).unwrap();
        let leftover = pki.join("trusted").join("leftover.der");
        std::fs::write(&leftover, b"x").unwrap();
        let outside = pki.join("precious.txt");
        std::fs::write(&outside, b"keep").unwrap();
        std::fs::write(
            pki.join(MARKER_FILE),
            format!("{}\n{}", leftover.display(), outside.display()),
        )
        .unwrap();

        purge_leftovers(&pki);
        assert!(!leftover.exists());
        assert!(outside.exists(), "files outside trusted/ are never deleted");
        assert!(!pki.join(MARKER_FILE).exists());
        let _ = std::fs::remove_dir_all(&pki);
    }

    #[test]
    fn an_unsecured_endpoint_has_no_certificate_to_lease() {
        let pki = scratch("none");
        assert!(
            trust_for_session(&pki, &ByteString::null())
                .unwrap()
                .is_none()
        );
        let _ = std::fs::remove_dir_all(&pki);
    }
}
