use super::{
    BackendMsg, BoundConnectionSource, BoundConnectorBuilder, CONNECT_TIMEOUT, CancellationToken,
    ClientBuilder, DISCOVERY_GUARD_TIMEOUT, EndpointDescription, EndpointSpec,
    GRACEFUL_CLOSE_TIMEOUT, HashMap, HashSet, IdentityToken, JoinHandle, MessageSecurityMode,
    NetworkBinding, Password, PathBuf, Session, SessionId, StatusCode, UaEndpointInfo,
    UaSecurityPolicy, UaSession, UserTokenPolicy, UserTokenType,
    auto_chose_unprotected_over_secure, build_discovery_client, purge_stale, resolve_security,
    security_error, security_summary, trust_for_session,
};
use super::{Event, EventKind, READ_MAX_ITEMS_PER_CALL, Sender};

pub(in crate::bff::ua) fn describe_connect_error(
    url: &str,
    pki: &std::path::Path,
    error: opcua::types::Error,
) -> BackendMsg {
    let msg = error.to_string();
    if certification_error(&msg) {
        BackendMsg::UaCertUntrusted {
            url: url.to_string(),
            detail: msg,
            pki: pki.display().to_string(),
        }
    } else {
        BackendMsg::UaConnectFailed {
            url: url.to_string(),
            detail: msg,
        }
    }
}

/// Spawn the session event loop and wait for the session to activate. `build()`
/// itself does not open a connection, so this is the first network step after
/// the endpoint description has been selected. A cancelled attempt closes the
/// channel gracefully like a failed one.
async fn activate_ua_session<C>(
    session: &Session,
    event_loop: opcua::client::SessionEventLoop<C>,
    url: &str,
    cancel: &CancellationToken,
) -> Result<JoinHandle<StatusCode>, BackendMsg>
where
    C: opcua::client::transport::Connector + Send + Sync + 'static,
{
    let mut event_loop = event_loop.spawn();
    let connected = tokio::select! {
        biased;
        _ = cancel.cancelled() => None,
        connected = tokio::time::timeout(CONNECT_TIMEOUT, session.wait_for_connection()) => {
            Some(connected)
        }
    };
    match connected {
        Some(Ok(true)) => Ok(event_loop),
        Some(Ok(false)) => {
            let _ = tokio::time::timeout(GRACEFUL_CLOSE_TIMEOUT, session.disconnect()).await;
            let _ = tokio::time::timeout(GRACEFUL_CLOSE_TIMEOUT, &mut event_loop).await;
            Err(BackendMsg::UaSessionClosedOnConnect {
                url: url.to_string(),
            })
        }
        Some(Err(_)) => {
            // Timeout activating the session: the transport is already connected
            // so the server expects a CloseSession/CloseSecureChannel. Shut the
            // channel down gracefully instead of aborting the loop mid-request.
            let _ = tokio::time::timeout(GRACEFUL_CLOSE_TIMEOUT, session.disconnect()).await;
            let _ = tokio::time::timeout(GRACEFUL_CLOSE_TIMEOUT, &mut event_loop).await;
            Err(BackendMsg::TimeoutActivateSession)
        }
        None => {
            let _ = tokio::time::timeout(GRACEFUL_CLOSE_TIMEOUT, session.disconnect()).await;
            let _ = tokio::time::timeout(GRACEFUL_CLOSE_TIMEOUT, &mut event_loop).await;
            Err(BackendMsg::ConnectCancelled)
        }
    }
}

pub(in crate::bff::ua) async fn connect(
    session_id: SessionId,
    connection_id: Option<crate::bff::model::RequestId>,
    endpoint: EndpointSpec,
    cancel: &CancellationToken,
    event_tx: &Sender<Event>,
) -> Result<UaSession, BackendMsg> {
    let url = endpoint.address.trim();
    if url.parse::<opclook_contract::ua_url::UaUrl>().is_err() {
        return Err(BackendMsg::UaUrlScheme);
    }
    let binding = endpoint.ua_bind.clone();

    // 1. Fetch the server's actual endpoints so we can pick a security policy
    //    and mode the server really offers instead of guessing blindly. The full
    //    descriptions are kept so the connect description can reuse the server's
    //    own user-token policies (CODESYS advertises "Anonymous", not the
    //    crate's hardcoded "anonymous", and rejects ActivateSession otherwise
    //    with BadIdentityTokenInvalid).
    let mut server_endpoints = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(BackendMsg::ConnectCancelled),
        found = discover_endpoints(url, binding.as_ref()) => found?,
    };
    let mut discovered = Vec::with_capacity(server_endpoints.len());
    server_endpoints.retain(|e| match e.endpoint_url.to_string().parse() {
        Ok(url) => {
            discovered.push(UaEndpointInfo {
                url,
                security_policy: crate::bff::model::AdvertisedPolicy::from_wire(
                    &e.security_policy_uri.to_string(),
                ),
                security_mode: crate::bff::ua::security::advertised_mode(e.security_mode),
            });
            true
        }
        Err(_) => {
            let _ = event_tx.send(Event {
                origin: crate::bff::model::Origin::Unsolicited,
                owner: session_id.into(),
                kind: EventKind::Log {
                    message: BackendMsg::InvalidDiscoveredUaUrl,
                },
            });
            false
        }
    });

    // 2. Resolve the requested policy/mode (Auto default) against what the
    //    server advertises. An unsupported explicit choice is a hard error: no
    //    silent fallback to None/other.
    let (policy_uri, mode) = resolve_security(
        endpoint.ua_security_policy,
        endpoint.ua_security_mode,
        &discovered,
    )
    .map_err(security_error)?;

    let policy_short = UaSecurityPolicy::from_advertised(&policy_uri)
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|| policy_uri.clone());

    // 3. Build the client with the PKI store and the user's trust preference.
    let pki = pki_dir();
    purge_stale(&pki);
    let mut builder = ClientBuilder::new()
        .application_name("OPCLook")
        .application_uri("urn:opclook")
        .product_uri("urn:opclook")
        .create_sample_keypair(true)
        // The library's own "trust unknown certificates" switch would store the
        // server certificate in `trusted` FOREVER. "Trust" in the connect dialog
        // means this session only, which `TrustLease` provides (see trust.rs).
        .trust_server_certs(false)
        // A trusted server is also exempt from the validity period, host name and
        // application URI checks (industrial devices often fail them); the key
        // length and the identity of the certificate are still checked.
        .verify_server_certs(!endpoint.ua_trust_server_cert)
        .pki_dir(pki.clone())
        .session_retry_limit(3);

    // 4. Session timeout (optional; the crate accepts a requested value and the
    //    server revises it, but the revised value is not exposed — see docs).
    if let Some(timeout_ms) = endpoint.ua_session_timeout {
        builder = builder.session_timeout(timeout_ms);
    }

    let client = build_client(builder)?;

    // 5. Reuse the selected server's full endpoint description, including its
    //    actual user-token policies.
    let description = endpoint_description_for(&server_endpoints, url, &policy_uri, mode);

    // 5b. "Trust the certificate" = trust for the lifetime of this session. The
    //     lease is dropped (and the certificate untrusted again) when the connect
    //     fails below or the session closes.
    let trust = if endpoint.ua_trust_server_cert {
        trust_for_session(&pki, &description.server_certificate)?
    } else {
        None
    };

    // 6. Validate identity against the selected endpoint, not a guessed
    //    correlation between message security and certificate support.
    let identity = resolve_identity_token(&endpoint, &description)?;

    // 7. Build the session (no network I/O), then start its event loop and wait
    //    for activation. The binding captured on the EndpointSpec is reused on
    //    every reconnect so a session never silently moves to another card.
    let (session, event_loop) = if let Some(binding) = binding {
        let (session, event_loop) = client
            .session_builder()
            .with_connector(BoundConnectionSource::new(binding))
            .connect_to_endpoint_directly(description)
            .map_err(|error| describe_connect_error(url, &pki, error))?
            .user_identity_token(identity)
            .build(client.certificate_store().clone())
            .map_err(|error| describe_connect_error(url, &pki, error))?;
        let event_loop = activate_ua_session(&session, event_loop, url, cancel).await?;
        (session, event_loop)
    } else {
        let (session, event_loop) = client
            .session_builder()
            .connect_to_endpoint_directly(description)
            .map_err(|error| describe_connect_error(url, &pki, error))?
            .user_identity_token(identity)
            .build(client.certificate_store().clone())
            .map_err(|error| describe_connect_error(url, &pki, error))?;
        let event_loop = activate_ua_session(&session, event_loop, url, cancel).await?;
        (session, event_loop)
    };

    let secure_was_available = auto_chose_unprotected_over_secure(
        endpoint.ua_security_policy,
        endpoint.ua_security_mode,
        mode,
        &discovered,
    );
    Ok(UaSession {
        session,
        event_loop,
        items: HashMap::new(),
        node_types: HashMap::new(),
        info: security_summary(url, &policy_short, mode, secure_was_available),
        session_id,
        connection_id,
        subscriptions: Vec::new(),
        publishing_interval_ms: 0,
        deadband: 0.0,
        applied_interval_ms: 0,
        applied_deadband: 0.0,
        monitored: HashMap::new(),
        poll_only: HashSet::new(),
        monitor_chunk: READ_MAX_ITEMS_PER_CALL,
        unfiltered: HashSet::new(),
        timed_out: false,
        trust,
        types: None,
    })
}

/// Choose the endpoint description the session connects with: the server's own
/// advertised entry for the resolved policy/mode, so its user-token policies
/// (and certificate) are reused, with the endpoint URL replaced by the
/// user-supplied one. Falls back to a minimal anonymous description when no
/// advertised entry matches.
///
/// The hand-built fallback pins `UserTokenPolicy::anonymous()`, whose
/// `policy_id` is the crate default `"anonymous"`. CODESYS advertises its
/// anonymous policy as `"Anonymous"` and answers ActivateSession with
/// `BadIdentityTokenInvalid` on a mismatch, so a matching advertised endpoint
/// must be preferred.
pub(in crate::bff::ua) fn endpoint_description_for(
    server_endpoints: &[EndpointDescription],
    url: &str,
    policy_uri: &str,
    mode: MessageSecurityMode,
) -> EndpointDescription {
    let mut description = server_endpoints
        .iter()
        .find(|e| e.security_policy_uri.as_ref() == policy_uri && e.security_mode == mode)
        .cloned()
        .unwrap_or_else(|| (url, policy_uri, mode, UserTokenPolicy::anonymous()).into());
    description.endpoint_url = url.into();
    description
}

/// Determine the OPC UA user identity token for a connect. Supports Anonymous,
/// UserName and (when the endpoint advertises Certificate tokens) X509 reusing
/// the client certificate from the PKI store.
pub(in crate::bff::ua) fn resolve_identity_token(
    endpoint: &EndpointSpec,
    description: &EndpointDescription,
) -> Result<IdentityToken, BackendMsg> {
    if endpoint.ua_use_certificate {
        if description
            .find_policy(UserTokenType::Certificate)
            .is_none()
        {
            return Err(BackendMsg::UaCertLoginUnsupported);
        }
        let pki = pki_dir();
        let cert_path = pki.join("own").join("cert.der");
        let key_path = pki.join("private").join("private.pem");
        if !cert_path.exists() || !key_path.exists() {
            return Err(BackendMsg::UaClientCertMissing {
                cert: cert_path.display().to_string(),
                key: key_path.display().to_string(),
            });
        }
        return IdentityToken::new_x509_path(&cert_path, &key_path).map_err(|e| {
            BackendMsg::UaClientCertLoadFailed {
                detail: e.to_string(),
            }
        });
    }

    if endpoint.user.trim().is_empty() {
        Ok(IdentityToken::Anonymous)
    } else {
        Ok(IdentityToken::UserName(
            endpoint.user.clone(),
            Password(endpoint.password.clone()),
        ))
    }
}

/// Best-effort detection of certificate-related connect failures so we can tell
/// the user to enable trust / import the certificate.
fn certification_error(message: &str) -> bool {
    let m = message.to_ascii_lowercase();
    m.contains("badcertificate") || m.contains("untrusted") || m.contains("certificate")
}

/// Fetches the server's advertised endpoints via `GetEndpoints`. The client is
/// built with a permissive trust setting so discovery works even when the
/// server cert is not yet trusted (GetEndpoints runs over an anonymous channel).
/// The full descriptions are returned so the connect description can reuse the
/// server's own user-token policies.
async fn discover_endpoints(
    url: &str,
    binding: Option<&NetworkBinding>,
) -> Result<Vec<EndpointDescription>, BackendMsg> {
    let client = build_discovery_client()?;
    let request = if let Some(binding) = binding {
        let connector = BoundConnectorBuilder::new(url, binding.clone());
        tokio::time::timeout(
            DISCOVERY_GUARD_TIMEOUT,
            client.get_server_endpoints_from_url(connector),
        )
        .await
    } else {
        tokio::time::timeout(
            DISCOVERY_GUARD_TIMEOUT,
            client.get_server_endpoints_from_url(url),
        )
        .await
    };
    let endpoints = request
        .map_err(|_| BackendMsg::TimeoutEndpoints {
            url: url.to_string(),
        })?
        .map_err(|e| BackendMsg::EndpointsFailed {
            url: url.to_string(),
            detail: e.to_string(),
        })?;

    Ok(endpoints)
}

/// Test-only override of the PKI store location (see `live_tests`).
#[cfg(test)]
pub(in crate::bff::ua) static TEST_PKI_DIR: std::sync::OnceLock<PathBuf> =
    std::sync::OnceLock::new();

pub(in crate::bff::ua) fn pki_dir() -> PathBuf {
    pki_dir_public()
}

/// Builds the client. The library reads the application key pair from the PKI
/// store here and, on the first run, creates it (certificate, then private key,
/// as two files). Two clients built at the same moment (the combined search
/// starts several, tests run many in parallel) could both see "missing", or one
/// could read a half-written pair; the loser then reported an error or failed
/// its first secure connect, and the next attempt (files complete by then)
/// worked. One lock makes the creation happen once, before any other client
/// looks at the files.
pub(in crate::bff::ua) fn build_client(
    builder: opcua::client::ClientBuilder,
) -> Result<opcua::client::Client, BackendMsg> {
    static BUILD: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = BUILD
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    builder
        .client()
        .map_err(|errors| BackendMsg::Raw(errors.join("; ")))
}

/// Process-wide redirect of the PKI store used by headless tests of the
/// application, so that they share neither the real store nor the working
/// directory's `opclook/pki`. The application never sets it.
static PKI_DIR_OVERRIDE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Redirect this process's PKI store to a private temporary directory (once;
/// later calls return the same directory).
pub fn use_private_pki_dir() -> PathBuf {
    PKI_DIR_OVERRIDE
        .get_or_init(|| {
            let dir = std::env::temp_dir().join(format!("opclook-test-pki-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            dir
        })
        .clone()
}

/// Public accessor for the PKI store path (the connect dialog / About dialog
/// display it). Path: `%APPDATA%\opclook\pki`.
pub fn pki_dir_public() -> PathBuf {
    // Live tests run many clients in one process: they share one scratch PKI
    // store instead of the user's real one.
    #[cfg(test)]
    if let Some(dir) = TEST_PKI_DIR.get() {
        return dir.clone();
    }
    if let Some(dir) = PKI_DIR_OVERRIDE.get() {
        return dir.clone();
    }
    // Unit tests that build a discovery client before live test setup must not
    // create certificates in the repository or the user's profile.
    #[cfg(test)]
    return use_private_pki_dir();
    #[cfg(not(test))]
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("opclook")
        .join("pki")
}

/// Best-effort read of the client certificate subject from the PKI store.
/// Returns `None` when the cert has not been generated yet (first connect) or
/// cannot be read. Used by the «О программе» dialog.
pub fn client_cert_subject() -> Option<String> {
    let cert_path = pki_dir().join("own").join("cert.der");
    if !cert_path.exists() {
        return None;
    }
    opcua::crypto::CertificateStore::read_cert(&cert_path)
        .ok()
        .map(|cert| cert.subject_name())
        .map(|s| {
            if s.trim().is_empty() {
                "—".to_string()
            } else {
                s
            }
        })
}

#[cfg(test)]
mod build_client_tests {
    #[test]
    fn tests_use_a_temporary_pki_store() {
        assert!(super::pki_dir_public().starts_with(std::env::temp_dir()));
    }

    use super::*;
    use opcua::client::ClientBuilder;
    use opcua::crypto::CertificateStore;
    use std::sync::{Arc, Barrier, Mutex};

    /// Collects the library's error lines (the only place where it reports that
    /// creating or reading the key pair went wrong).
    struct Capture(Mutex<Vec<String>>);

    impl log::Log for Capture {
        fn enabled(&self, metadata: &log::Metadata) -> bool {
            metadata.level() <= log::Level::Error
        }
        fn log(&self, record: &log::Record) {
            if self.enabled(record.metadata()) {
                let mut lines = self.0.lock().unwrap_or_else(|e| e.into_inner());
                lines.push(record.args().to_string());
            }
        }
        fn flush(&self) {}
    }

    /// The first run creates the application key pair; clients built at the same
    /// moment must create it once and all find it complete. Without the lock the
    /// losers tried to create it again ("Certificate creation failed ... already
    /// exists") or read one half written.
    #[test]
    fn clients_built_concurrently_on_a_fresh_store_create_the_key_pair_once() {
        static CAPTURE: Capture = Capture(Mutex::new(Vec::new()));
        // Another test in this process may have installed a logger already; then
        // the lines cannot be observed and only the files are checked.
        let observing = log::set_logger(&CAPTURE).is_ok();
        if observing {
            log::set_max_level(log::LevelFilter::Error);
        }
        let dir = std::env::temp_dir().join(format!("opclook-build-client-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let threads = 8;
        let barrier = Arc::new(Barrier::new(threads));
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                let dir = dir.clone();
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    let builder = ClientBuilder::new()
                        .application_name("OPCLook")
                        .application_uri("urn:opclook")
                        .product_uri("urn:opclook")
                        .create_sample_keypair(true)
                        .pki_dir(dir.clone());
                    build_client(builder)
                        .map_err(|e| crate::i18n::backend_text(crate::i18n::Lang::En, &e))?;
                    let store = CertificateStore::new(&dir);
                    store.read_own_cert()?;
                    CertificateStore::read_pkey(&store.own_private_key_path())?;
                    Ok::<(), String>(())
                })
            })
            .collect();
        for handle in handles {
            let result = handle.join().expect("client thread");
            assert!(result.is_ok(), "{result:?}");
        }
        if observing {
            // Other tests of this process log too (connection errors); only the
            // key-pair lines belong here.
            let lines: Vec<String> = CAPTURE
                .0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .filter(|line| {
                    line.contains("Certificate creation failed")
                        || line.contains("missing its application instance certificate")
                })
                .cloned()
                .collect();
            assert!(lines.is_empty(), "the library reported: {lines:?}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
