use opclook_contract::model::{
    BackendMsg, CommandKind, DataValue, EndpointSpec, Event, EventKind, ItemKey, ItemSpec,
    Operation, Protocol, QualityKind, SessionId, UaSecurityMode, UaSecurityPolicy, Value,
    WriteRequest,
};
use opclook_contract::net::NetworkBinding;
use opclook_engine::{DispatchError, Due, Engine};
use opclook_locale::{Lang, backend_text};
use serde_json::{Value as Json, json};
use std::future::Future;
use std::io::{self, Write};
use std::net::IpAddr;
use std::sync::mpsc::{self, Receiver};
use std::task::Poll;
use std::time::{Duration, Instant};

const HELP: &str = "opclook-cli — headless OPC DA/UA client\n\nUsage: opclook-cli [options] COMMAND [arguments]\n\nEndpoints (choose one):\n  --ua URL                  OPC UA endpoint\n  --da HOST PROGID          OPC DA server (Windows)\n\nOptions:\n  --timeout SECONDS         Per-operation deadline (default 30)\n  --bind IP                 Local UA source address\n  --if-index INDEX          Interface index for the source address\n  --scope INDEX             IPv6 scope ID\n  --policy POLICY           UA policy (Auto, None, Basic256Sha256, etc.)\n  --mode MODE               UA mode (Auto, None, Sign, SignAndEncrypt)\n  --trust-server-cert       Trust the UA server certificate\n  --client-cert             Use the client certificate from the PKI store\n  --session-timeout-ms N    Requested UA session timeout in milliseconds\n  --user-env NAME           Read username from an environment variable\n  --password-env NAME       Read password from an environment variable\n  --interval SECONDS        Watch polling interval (default 1)\n  --interval-ms N           Per-connection interval in milliseconds\n  --update-mode MODE        subscription or poll\n  --poll-freshness MODE     cache or source\n  --count N                 Stop watch after N value records (default: Ctrl+C)\n  --poll                    Watch by periodic reads instead of UA subscription\n  --read-after-write [true|false]  Observe after accepted UA write\n  --json                    Emit JSON or JSONL (the default)\n  --help                    Print this help\n\nCommands:\n  browse [NODE]             Browse a node (default: root)\n  read NODE                 Add and read a node\n  write NODE TYPE VALUE     Write bool, i64, u64, f64 or string\n  watch NODE                Subscribe/poll until count or Ctrl+C\n\n64-bit integers are decimal strings. Non-finite f64 values are strings NaN,\nInfinity and -Infinity. Credentials are read from the environment.\n";
const MAX_ARG: usize = 4096;
const MAX_JSON_BUDGET: usize = 1_000_000;
const MAX_DEPTH: usize = 32;
const OUTPUT_CLOSED: &str = "stdout closed";
struct Options {
    endpoint: EndpointSpec,
    timeout: Duration,
    interval: Duration,
    count: Option<usize>,
    poll: bool,
    read_after_write: bool,
    command: Vec<String>,
}
fn value_arg(args: &[String], i: &mut usize, name: &str) -> Result<String, String> {
    *i += 1;
    args.get(*i)
        .cloned()
        .ok_or_else(|| format!("{name} requires a value"))
}
fn positive(text: &str, name: &str, max: u64) -> Result<u64, String> {
    let n = text.parse::<u64>().map_err(|_| format!("invalid {name}"))?;
    if n == 0 || n > max {
        return Err(format!("{name} must be between 1 and {max}"));
    }
    Ok(n)
}
fn parse(args: Vec<String>) -> Result<Option<Options>, String> {
    if args.len() > 64 || args.iter().any(|s| s.len() > MAX_ARG) {
        return Err("too many or oversized arguments".into());
    }
    let mut endpoint = None;
    let mut timeout = Duration::from_secs(30);
    let mut interval = Duration::from_secs(1);
    let mut count = None;
    let mut poll = false;
    let mut read_after_write: Option<bool> = None;
    let mut update_mode = None;
    let mut poll_freshness = None;
    let mut acquisition_requested = false;
    let mut interval_seconds_used = false;
    let mut interval_ms_used = false;
    let mut policy = None;
    let mut mode = None;
    let mut trust = false;
    let mut client_cert = false;
    let mut session_timeout = None;
    let mut user_env = None;
    let mut password_env = None;
    let mut bind = None;
    let mut if_index = None;
    let mut scope = None;
    let mut i = 0;
    while i < args.len() && args[i].starts_with('-') {
        let key = args[i].as_str();
        match key {
            "--ua" => {
                if endpoint.is_some() {
                    return Err("select one endpoint".into());
                }
                let url = value_arg(&args, &mut i, key)?;
                if url.parse::<opclook_contract::ua_url::UaUrl>().is_err() {
                    return Err("invalid UA endpoint URL".into());
                }
                endpoint = Some(EndpointSpec::ua(url));
            }
            "--da" => {
                if endpoint.is_some() {
                    return Err("select one endpoint".into());
                }
                let host = value_arg(&args, &mut i, key)?;
                let progid = value_arg(&args, &mut i, key)?;
                endpoint = Some(EndpointSpec::da(host, progid));
            }
            "--timeout" => {
                timeout = Duration::from_secs(positive(
                    &value_arg(&args, &mut i, key)?,
                    "timeout",
                    86_400,
                )?)
            }
            "--interval" => {
                interval_seconds_used = true;
                interval = Duration::from_secs(positive(
                    &value_arg(&args, &mut i, key)?,
                    "interval",
                    3600,
                )?)
            }
            "--count" => {
                count =
                    Some(positive(&value_arg(&args, &mut i, key)?, "count", 1_000_000)? as usize)
            }
            "--policy" => {
                policy = Some(
                    UaSecurityPolicy::from_stored(&value_arg(&args, &mut i, key)?)
                        .ok_or("unknown security policy")?,
                )
            }
            "--mode" => {
                mode = Some(
                    UaSecurityMode::from_stored(&value_arg(&args, &mut i, key)?)
                        .ok_or("unknown security mode")?,
                )
            }
            "--trust-server-cert" => trust = true,
            "--client-cert" => client_cert = true,
            "--session-timeout-ms" => {
                session_timeout = Some(positive(
                    &value_arg(&args, &mut i, key)?,
                    "session timeout",
                    3_600_000,
                )? as u32)
            }
            "--user-env" => user_env = Some(value_arg(&args, &mut i, key)?),
            "--password-env" => password_env = Some(value_arg(&args, &mut i, key)?),
            "--bind" => {
                bind = Some(
                    value_arg(&args, &mut i, key)?
                        .parse::<IpAddr>()
                        .map_err(|_| "invalid bind address")?,
                )
            }
            "--if-index" => {
                if_index = std::num::NonZeroU32::new(positive(
                    &value_arg(&args, &mut i, key)?,
                    "interface index",
                    u32::MAX as u64,
                )? as u32)
            }
            "--scope" => {
                scope = std::num::NonZeroU32::new(positive(
                    &value_arg(&args, &mut i, key)?,
                    "scope",
                    u32::MAX as u64,
                )? as u32)
            }
            "--json" => {}
            "--poll" => {
                poll = true;
                acquisition_requested = true;
            }
            "--update-mode" => {
                update_mode = Some(match value_arg(&args, &mut i, key)?.as_str() {
                    "subscription" => opclook_contract::model::UpdateMode::Subscription,
                    "poll" => opclook_contract::model::UpdateMode::Poll,
                    _ => return Err("unknown update mode".into()),
                });
                acquisition_requested = true;
            }
            "--interval-ms" => {
                interval_ms_used = true;
                interval = Duration::from_millis(positive(
                    &value_arg(&args, &mut i, key)?,
                    "interval-ms",
                    opclook_contract::model::ACQUISITION_INTERVAL_MAX_MS,
                )?);
                acquisition_requested = true;
            }
            "--poll-freshness" => {
                poll_freshness = Some(match value_arg(&args, &mut i, key)?.as_str() {
                    "cache" => opclook_contract::model::PollFreshness::CacheAllowed,
                    "source" => opclook_contract::model::PollFreshness::FreshSource,
                    _ => return Err("unknown poll freshness".into()),
                });
                acquisition_requested = true;
            }
            "--read-after-write" => {
                read_after_write = Some(match args.get(i + 1).map(String::as_str) {
                    Some("true") => {
                        i += 1;
                        true
                    }
                    Some("false") => {
                        i += 1;
                        false
                    }
                    Some(next)
                        if next.starts_with('-')
                            || matches!(next, "browse" | "read" | "write" | "watch") =>
                    {
                        true
                    }
                    None => true,
                    Some(_) => return Err("--read-after-write expects true or false".into()),
                });
                acquisition_requested = true;
            }
            "--help" | "-h" => return Ok(None),
            _ => return Err("unknown option".into()),
        }
        i += 1;
    }
    let mut endpoint = endpoint.ok_or("an endpoint is required")?;
    if interval_seconds_used && interval_ms_used {
        return Err("--interval and --interval-ms conflict".into());
    }
    if poll && update_mode == Some(opclook_contract::model::UpdateMode::Subscription) {
        return Err("--poll conflicts with subscription update mode".into());
    }
    if acquisition_requested {
        let mut config = if endpoint.protocol == Protocol::OpcUa {
            opclook_contract::model::AcquisitionConfig::ua_default()
        } else {
            opclook_contract::model::AcquisitionConfig::da_available()
        };
        // Preserve CLI's historic default; both Poll spellings use one policy.
        config.read_after_write = false;
        if poll {
            config.update_mode = opclook_contract::model::UpdateMode::Poll;
        }
        if let Some(mode) = update_mode {
            config.update_mode = mode;
        }
        if let Some(freshness) = poll_freshness {
            config.poll_freshness = freshness;
        }
        if let Some(readback) = read_after_write {
            config.read_after_write = readback;
        }
        config.interval_ms = interval
            .as_millis()
            .try_into()
            .map_err(|_| "interval too large")?;
        endpoint.acquisition = Some(config);
        opclook_engine::validate_acquisition(&endpoint)
            .map_err(|error| format!("invalid acquisition: {error:?}"))?;
    }
    let read_after_write = endpoint
        .acquisition
        .as_ref()
        .is_some_and(|c| c.read_after_write);
    poll = endpoint
        .acquisition
        .as_ref()
        .is_some_and(|c| c.update_mode == opclook_contract::model::UpdateMode::Poll);

    if endpoint.protocol == Protocol::OpcDa
        && (bind.is_some()
            || if_index.is_some()
            || scope.is_some()
            || policy.is_some()
            || mode.is_some()
            || trust
            || client_cert
            || session_timeout.is_some()
            || user_env.is_some()
            || password_env.is_some())
    {
        return Err("UA security and binding options require --ua".into());
    }
    if bind.is_none() && (if_index.is_some() || scope.is_some()) {
        return Err("--if-index and --scope require --bind".into());
    }
    if let Some(ip) = bind {
        endpoint.ua_bind = Some(NetworkBinding::new(
            ip,
            if ip.is_ipv4() { if_index } else { None },
            if ip.is_ipv6() { if_index } else { None },
            scope,
        ));
    }
    endpoint.ua_security_policy = policy.unwrap_or_default();
    endpoint.ua_security_mode = mode.unwrap_or_default();
    endpoint.ua_trust_server_cert = trust;
    endpoint.ua_use_certificate = client_cert;
    endpoint.ua_session_timeout = session_timeout;
    if let Some(name) = user_env {
        endpoint.user =
            std::env::var(name).map_err(|_| "username environment variable is unavailable")?;
    }
    if let Some(name) = password_env {
        endpoint.password =
            std::env::var(name).map_err(|_| "password environment variable is unavailable")?;
    }
    let command = args[i..].to_vec();
    if command.is_empty() {
        return Err("a command is required".into());
    }
    Ok(Some(Options {
        endpoint,
        timeout,
        interval,
        count,
        poll,
        read_after_write,
        command,
    }))
}
fn redact(mut text: String, endpoint: &EndpointSpec) -> String {
    let mut secrets = [&endpoint.user, &endpoint.password];
    secrets.sort_by_key(|secret| std::cmp::Reverse(secret.len()));
    for secret in secrets {
        if !secret.is_empty() {
            text = text.replace(secret, "[redacted]");
        }
    }
    text
}
fn message(msg: &BackendMsg, endpoint: &EndpointSpec) -> String {
    redact(backend_text(Lang::En, msg), endpoint)
}
fn send(
    engine: &mut Engine<()>,
    protocol: Protocol,
    kind: CommandKind,
) -> Result<opclook_contract::model::RequestId, String> {
    let dispatch = match kind {
        CommandKind::Browse {
            node_id,
            filter,
            view_id,
        } => engine.execute_action(
            opclook_contract::actions::DomainAction::Browse {
                session: SessionId::FIRST,
                protocol,
                node_id,
                filter,
                view_id,
            },
            (),
        ),
        CommandKind::Write { request } => engine.execute_action(
            opclook_contract::actions::DomainAction::Write {
                session: SessionId::FIRST,
                protocol,
                request,
            },
            (),
        ),
        kind => engine.send(SessionId::FIRST, protocol, kind, ()),
    };
    dispatch.map_err(dispatch_error)
}
fn dispatch_error(error: DispatchError) -> String {
    match error {
        DispatchError::InvalidActionPayload(reason) => {
            format!("action payload exceeds limits: {reason:?}")
        }
        DispatchError::InvalidAcquisition(error) => format!("invalid acquisition: {error:?}"),
        DispatchError::Exhausted => "request identifiers exhausted".into(),
        DispatchError::ReadBusy => {
            opclook_locale::tr(Lang::En, opclook_locale::Tr::ReadBusy).into()
        }
        DispatchError::ReadEmpty => {
            opclook_locale::tr(Lang::En, opclook_locale::Tr::ReadEmpty).into()
        }
        DispatchError::ItemKeyReused => {
            opclook_locale::tr(Lang::En, opclook_locale::Tr::ItemKeyReused).into()
        }
        DispatchError::ItemKeyLimit => {
            opclook_locale::tr(Lang::En, opclook_locale::Tr::ItemKeyLimit).into()
        }
        DispatchError::SessionLimit => "UA session limit reached".into(),
        DispatchError::ActionUnavailable(reason) => {
            let key = match reason {
                opclook_engine::ActionUnavailable::RequestIdsExhausted => {
                    opclook_locale::Tr::RequestIdsExhausted
                }
                opclook_engine::ActionUnavailable::ReadPending => {
                    opclook_locale::Tr::ActionReadPending
                }
                opclook_engine::ActionUnavailable::SessionMissing => {
                    opclook_locale::Tr::ActionSessionMissing
                }
                opclook_engine::ActionUnavailable::SessionNotConnected => {
                    opclook_locale::Tr::ActionSessionNotConnected
                }
                opclook_engine::ActionUnavailable::ProtocolMismatch => {
                    opclook_locale::Tr::ActionProtocolMismatch
                }
            };
            opclook_locale::tr(Lang::En, key).into()
        }
        DispatchError::Closed { cause, .. } => {
            opclook_locale::send_error_text(Lang::En, cause).into()
        }
    }
}
fn dispatch_due(engine: &mut Engine<()>, interval: Duration) -> Result<(), String> {
    for due in engine.due(Instant::now(), interval, Some(Duration::from_secs(30))) {
        match due {
            Due::Read {
                session,
                protocol,
                keys,
                permit,
            } if session == SessionId::FIRST => {
                engine
                    .send_due_read(session, protocol, keys, permit, ())
                    .map_err(dispatch_error)?;
            }
            Due::ReadUnavailable { session } if session == SessionId::FIRST => {
                return Err("request identifiers exhausted".into());
            }
            Due::Test { session, protocol } if session == SessionId::FIRST => {
                send(engine, protocol, CommandKind::Test)?;
            }
            _ => {}
        }
    }
    Ok(())
}
fn interrupt_receiver() -> Result<Receiver<Result<(), String>>, String> {
    let (tx, rx) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("opclook-cli-signal".into())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    let _ = ready_tx.send(Err(format!("could not start signal runtime: {error}")));
                    return;
                }
            };
            let result = runtime.block_on(async {
                let signal = tokio::signal::ctrl_c();
                tokio::pin!(signal);
                let mut ready = false;
                std::future::poll_fn(|cx| match signal.as_mut().poll(cx) {
                    Poll::Pending => {
                        if !ready {
                            let _ = ready_tx.send(Ok(()));
                            ready = true;
                        }
                        Poll::Pending
                    }
                    Poll::Ready(result) => {
                        if !ready {
                            let _ = ready_tx
                                .send(result.as_ref().map(|_| ()).map_err(|e| e.to_string()));
                        }
                        Poll::Ready(result)
                    }
                })
                .await
            });
            let _ = tx.send(result.map_err(|e| format!("signal handler failed: {e}")));
        })
        .map_err(|e| format!("could not start signal handler: {e}"))?;
    ready_rx
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| "signal handler did not start".to_string())??;
    Ok(rx)
}
fn stopped(stop: &Receiver<Result<(), String>>) -> Result<bool, String> {
    match stop.try_recv() {
        Ok(Ok(())) => Ok(true),
        Ok(Err(error)) => Err(error),
        Err(mpsc::TryRecvError::Empty) => Ok(false),
        Err(mpsc::TryRecvError::Disconnected) => Err("signal handler stopped".into()),
    }
}
fn next_event_accepted(
    engine: &mut Engine<()>,
    deadline: Instant,
    stop: &Receiver<Result<(), String>>,
) -> Result<(Event, Option<opclook_engine::PendingRequest<()>>), String> {
    loop {
        if stopped(stop)? {
            return Err("interrupted".into());
        }
        if Instant::now() >= deadline {
            return Err("operation timed out".into());
        }
        if let Some(event) = engine.try_recv() {
            if let Ok(pending) = engine.accept(&event) {
                return Ok((event, pending));
            }
        } else {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
fn next_event(
    engine: &mut Engine<()>,
    deadline: Instant,
    stop: &Receiver<Result<(), String>>,
) -> Result<Event, String> {
    next_event_accepted(engine, deadline, stop).map(|(event, _)| event)
}

/// A Read's terminal result is authoritative, including missing/duplicate keys.
fn read_response(
    engine: &mut Engine<()>,
    request: opclook_contract::model::RequestId,
    timeout: Duration,
    stop: &Receiver<Result<(), String>>,
    endpoint: &EndpointSpec,
) -> Result<opclook_contract::model::ReadTerminal, String> {
    let deadline = checked_deadline(Instant::now(), timeout)?;
    let mut timed_out = false;
    loop {
        if stopped(stop)? {
            engine.discard_pending(SessionId::FIRST, request);
            return Err("interrupted".into());
        }
        if !timed_out && Instant::now() >= deadline {
            timed_out = engine.timeout_read_request(SessionId::FIRST, request, timeout.as_secs());
            if !timed_out {
                return Err("operation timed out".into());
            }
        }
        let Some(event) = engine.try_recv() else {
            std::thread::sleep(Duration::from_millis(10));
            continue;
        };
        let pending = match engine.accept(&event) {
            Ok(pending) => pending,
            Err(()) => continue,
        };
        if event.origin == opclook_contract::model::Origin::Reply(request) {
            return pending
                .and_then(|p| {
                    p.read_terminals
                        .into_iter()
                        .find(|terminal| terminal.key == ItemKey::FIRST)
                })
                .ok_or_else(|| "read returned no terminal outcome".into());
        }
        if let EventKind::Disconnected { reason } = event.kind {
            return Err(message(&reason, endpoint));
        }
    }
}

fn checked_deadline(now: Instant, timeout: Duration) -> Result<Instant, String> {
    now.checked_add(timeout)
        .ok_or_else(|| "operation timeout is too large".to_string())
}

fn response(
    engine: &mut Engine<()>,
    request: opclook_contract::model::RequestId,
    timeout: Duration,
    stop: &Receiver<Result<(), String>>,
    endpoint: &EndpointSpec,
) -> Result<EventKind, String> {
    let deadline = checked_deadline(Instant::now(), timeout)?;
    loop {
        let event = next_event(engine, deadline, stop)?;
        if event.origin == opclook_contract::model::Origin::Reply(request) {
            return match event.kind {
                EventKind::Error { message: err, .. } => Err(message(&err, endpoint)),
                kind => Ok(kind),
            };
        }
        if let EventKind::Disconnected { reason } = event.kind {
            return Err(message(&reason, endpoint));
        }
    }
}
/// Preserve at most one bounded early watch value until its Add succeeds.
fn add_response(
    engine: &mut Engine<()>,
    request: opclook_contract::model::RequestId,
    timeout: Duration,
    stop: &Receiver<Result<(), String>>,
    endpoint: &EndpointSpec,
    capture_early: bool,
) -> Result<(EventKind, Option<Result<DataValue, String>>), String> {
    let deadline = checked_deadline(Instant::now(), timeout)?;
    let mut early = None;
    loop {
        let (event, _) = next_event_accepted(engine, deadline, stop)?;
        if event.origin == opclook_contract::model::Origin::Reply(request) {
            return match event.kind {
                EventKind::Error {
                    message: reason, ..
                } => Err(message(&reason, endpoint)),
                kind => Ok((kind, early)),
            };
        }
        if let EventKind::Disconnected { reason } = &event.kind {
            return Err(message(reason, endpoint));
        }
        if capture_early && matches!(event.origin, opclook_contract::model::Origin::Session(_)) {
            if let EventKind::Values { values } = event.kind {
                for (key, value) in values {
                    if key == ItemKey::FIRST
                        && !value.quality.waiting_for_initial_data(endpoint.protocol)
                    {
                        // Enforce the existing JSON value budget before retaining a copy.
                        early = Some(data_json(&value, endpoint).map(|_| value));
                        break;
                    }
                }
            }
        }
    }
}

fn write_response(
    engine: &mut Engine<()>,
    request: opclook_contract::model::RequestId,
    timeout: Duration,
    stop: &Receiver<Result<(), String>>,
) -> Result<opclook_contract::model::WriteOutcome, opclook_contract::model::BackendMsg> {
    use opclook_contract::model::{Origin, WriteOutcome};
    let deadline = match checked_deadline(Instant::now(), timeout) {
        Ok(deadline) => deadline,
        Err(error) => return Ok(WriteOutcome::unknown(error)),
    };
    loop {
        let event = match next_event(engine, deadline, stop) {
            Ok(event) => event,
            Err(error) => return Ok(WriteOutcome::unknown(error)),
        };
        if event.origin == Origin::Reply(request) {
            return match event.kind {
                EventKind::Wrote { key, result } if key == ItemKey::FIRST => result,
                EventKind::Wrote { .. } => {
                    Ok(WriteOutcome::unknown("write reply has unexpected item key"))
                }
                EventKind::Error { message, .. } => WriteOutcome::from_dispatch_error(message),
                _ => Ok(WriteOutcome::unknown("unexpected write response")),
            };
        }
        if let EventKind::Disconnected { reason } = event.kind {
            return Ok(WriteOutcome::unknown_because(reason));
        }
    }
}

fn send_explicit_due(
    engine: &mut Engine<()>,
    interval: Duration,
    kind: opclook_engine::ReadDemandKind,
) -> Result<opclook_contract::model::RequestId, String> {
    for due in engine.due(Instant::now(), interval, None) {
        match due {
            Due::Read {
                session,
                protocol,
                keys,
                permit,
            } if session == SessionId::FIRST => {
                let demand = opclook_engine::ReadDemand {
                    key: ItemKey::FIRST,
                    kind,
                };
                if !engine.reservation_covers(session, permit, demand) {
                    return Err("scheduled read does not cover requested demand".into());
                }
                return engine
                    .send_due_read(session, protocol, keys, permit, ())
                    .map_err(dispatch_error);
            }
            Due::ReadUnavailable { .. } => return Err("read request identifiers exhausted".into()),
            _ => {}
        }
    }
    Err("read-back or refresh could not be scheduled".into())
}

fn encode(value: &Value, depth: usize, remaining: &mut usize) -> Result<Json, String> {
    if depth > MAX_DEPTH {
        return Err("value exceeds JSON output limit".into());
    }
    let content = match value {
        Value::Bytes(bytes) => bytes.len(),
        Value::Str(text) | Value::Other(text) | Value::NodeId(text) => text.len(),
        Value::QualifiedName { name, .. } => name.len(),
        Value::LocalizedText { locale, text } => locale.len().saturating_add(text.len()),
        Value::Struct { type_name, fields } => {
            fields.iter().fold(type_name.len(), |size, (name, _)| {
                size.saturating_add(name.len())
            })
        }
        _ => 0,
    };
    let cost = content.saturating_add(10);
    if cost > *remaining {
        return Err("value exceeds JSON output limit".into());
    }
    *remaining -= cost;
    Ok(match value {
        Value::Empty => json!({"type":"empty"}),
        Value::Bool(v) => json!({"type":"bool","value":v}),
        Value::I64(v) => json!({"type":"i64","value":v.to_string()}),
        Value::U64(v) => json!({"type":"u64","value":v.to_string()}),
        Value::F64(v) => {
            let n = if v.is_nan() {
                json!("NaN")
            } else if *v == f64::INFINITY {
                json!("Infinity")
            } else if *v == f64::NEG_INFINITY {
                json!("-Infinity")
            } else {
                json!(v)
            };
            json!({"type":"f64","value":n})
        }
        Value::Str(v) => json!({"type":"string","value":v}),
        Value::Bytes(v) => json!({"type":"bytes","value":v}),
        Value::DateTime(v) => json!({"type":"datetime","value":v.to_rfc3339()}),
        Value::NodeId(v) => json!({"type":"node_id","value":v}),
        Value::QualifiedName {
            namespace_index,
            name,
        } => json!({"type":"qualified_name","namespace_index":namespace_index,"name":name}),
        Value::LocalizedText { locale, text } => {
            json!({"type":"localized_text","locale":locale,"text":text})
        }
        Value::Other(v) => json!({"type":"other","value":v}),
        Value::Array(values) => {
            let mut out = Vec::new();
            for v in values {
                out.push(encode(v, depth + 1, remaining)?);
            }
            json!({"type":"array","value":out})
        }
        Value::Struct { type_name, fields } => {
            let mut out = Vec::new();
            for (name, v) in fields {
                out.push(json!({"name":name,"value":encode(v,depth+1,remaining)?}));
            }
            json!({"type":"struct","type_name":type_name,"fields":out})
        }
    })
}
fn value_times_json(times: &opclook_contract::model::ValueTimes) -> Json {
    let origin = match times.origin {
        opclook_contract::model::ValueOrigin::Unknown => "unknown",
        opclook_contract::model::ValueOrigin::Read => "read",
        opclook_contract::model::ValueOrigin::Subscription => "subscription",
    };
    json!({"ua_source":times.ua_source.map(|v| v.to_rfc3339()), "ua_server":times.ua_server.map(|v| v.to_rfc3339()), "da_timestamp":times.da_timestamp.map(|v| v.to_rfc3339()), "received_at":times.received_at.map(|v| v.to_rfc3339()), "origin":origin, "source_clock":times.source_clock.map(|clock| clock.0)})
}

fn data_json(data: &DataValue, endpoint: &EndpointSpec) -> Result<Json, String> {
    if data.quality.text.len() > MAX_JSON_BUDGET {
        return Err("quality text exceeds JSON output limit".into());
    }
    let quality = match data.quality.kind {
        QualityKind::Good => "good",
        QualityKind::Uncertain => "uncertain",
        QualityKind::Bad => "bad",
    };
    Ok(
        json!({"value":encode(&data.value,0,&mut MAX_JSON_BUDGET.clone())?,"quality":{"code":data.quality.code,"kind":quality,"text":data.quality.text},"timestamp":data.timestamp.map(|v| v.to_rfc3339()),"times":value_times_json(&data.times),"reason":data.reason.as_ref().map(|reason| message(reason,endpoint))}),
    )
}

fn read_terminal_json(
    terminal: &opclook_contract::model::ReadTerminal,
    endpoint: &EndpointSpec,
) -> Json {
    use opclook_contract::model::{ReadNeed, ReadTerminalStatus};
    let needs: Vec<_> = terminal
        .needs
        .iter()
        .map(|need| match need {
            ReadNeed::Initial => "initial",
            ReadNeed::Poll => "poll",
            ReadNeed::ExplicitRefresh => "refresh",
            ReadNeed::WriteReadback => "readback",
            ReadNeed::Direct => "direct",
        })
        .collect();
    let (status, detail) = match &terminal.status {
        ReadTerminalStatus::Received(value) => (
            if value.reason.is_none() && value.quality.kind == QualityKind::Good {
                "received"
            } else {
                "unusable"
            },
            value.reason.as_ref().map(|v| message(v, endpoint)),
        ),
        ReadTerminalStatus::ReceivedMetadata {
            quality, reason, ..
        } => (
            if reason.is_none() && quality.kind == QualityKind::Good {
                "received"
            } else {
                "unusable"
            },
            reason.as_ref().map(|v| message(v, endpoint)),
        ),
        ReadTerminalStatus::InitialPush {
            quality, reason, ..
        } => (
            if reason.is_none() && quality.kind == QualityKind::Good {
                "initial_push"
            } else {
                "unusable"
            },
            reason.as_ref().map(|v| message(v, endpoint)),
        ),
        ReadTerminalStatus::MissingResult => ("missing_result", None),
        ReadTerminalStatus::ServiceError(error) => {
            ("service_error", Some(message(error, endpoint)))
        }
        ReadTerminalStatus::Timeout { seconds } => ("timeout", Some(seconds.to_string())),
        ReadTerminalStatus::Cancelled(reason) => ("cancelled", Some(format!("{reason:?}"))),
        ReadTerminalStatus::DispatchFailed(error) => ("dispatch_failed", Some(error.to_string())),
        ReadTerminalStatus::Unavailable => ("unavailable", None),
        ReadTerminalStatus::UnexpectedReply => ("unexpected_reply", None),
    };
    let metadata = match &terminal.status {
        ReadTerminalStatus::Received(value) => Some((&value.quality, value.timestamp)),
        ReadTerminalStatus::InitialPush {
            quality, timestamp, ..
        }
        | ReadTerminalStatus::ReceivedMetadata {
            quality, timestamp, ..
        } => Some((quality, *timestamp)),
        _ => None,
    };
    let quality = metadata.map(|(q, timestamp)| json!({ "code":q.code, "kind":match q.kind { QualityKind::Good => "good", QualityKind::Uncertain => "uncertain", QualityKind::Bad => "bad" }, "text":redact(q.text.to_string(), endpoint), "timestamp":timestamp.map(|t| t.to_rfc3339()) }));
    let times = match &terminal.status {
        ReadTerminalStatus::Received(value) => Some(&value.times),
        ReadTerminalStatus::InitialPush { times, .. }
        | ReadTerminalStatus::ReceivedMetadata { times, .. } => Some(times),
        _ => None,
    };
    json!({"request_id":terminal.request_id.map(|id| id.get()), "item_key":terminal.key.get(), "needs":needs, "status":status, "detail":detail, "quality":quality, "times":times.map(value_times_json)})
}

fn terminal_data(
    terminal: &opclook_contract::model::ReadTerminal,
    endpoint: &EndpointSpec,
) -> Result<DataValue, String> {
    use opclook_contract::model::ReadTerminalStatus;
    match &terminal.status {
        ReadTerminalStatus::Received(data) => Ok(data.clone()),
        ReadTerminalStatus::ServiceError(reason) => Err(message(reason, endpoint)),
        ReadTerminalStatus::MissingResult => Err("server returned no value".into()),
        ReadTerminalStatus::Timeout { .. } => Err("operation timed out".into()),
        ReadTerminalStatus::Cancelled(reason) => Err(format!("read cancelled: {reason:?}")),
        ReadTerminalStatus::DispatchFailed(error) => Err(error.to_string()),
        ReadTerminalStatus::Unavailable => Err("read unavailable".into()),
        ReadTerminalStatus::UnexpectedReply => Err("invalid read response".into()),
        ReadTerminalStatus::InitialPush { .. } | ReadTerminalStatus::ReceivedMetadata { .. } => {
            Err("expected a read reply".into())
        }
    }
}

fn write_outcome_json(
    node: &str,
    outcome: &opclook_contract::model::WriteOutcome,
    endpoint: &EndpointSpec,
) -> Json {
    use opclook_contract::model::{NativeWriteStatus, WriteDisposition};
    let disposition = match outcome.disposition {
        WriteDisposition::Accepted => "accepted",
        WriteDisposition::AcceptedAsynchronously => "accepted_asynchronously",
        WriteDisposition::Uncertain => "uncertain",
        WriteDisposition::Rejected => "rejected",
        WriteDisposition::Unknown => "unknown",
    };
    let raw = outcome.raw.map(|status| match status {
        NativeWriteStatus::Ua(code) => json!({"protocol":"ua", "scope":"item", "code":code}),
        NativeWriteStatus::Da(code) => json!({"protocol":"da", "scope":"item", "code":code as u32}),
        NativeWriteStatus::UaService(code) => {
            json!({"protocol":"ua", "scope":"service", "code":code})
        }
        NativeWriteStatus::DaService(code) => {
            json!({"protocol":"da", "scope":"service", "code":code as u32})
        }
    });
    json!({"node_id":node,"written":outcome.accepted(),"accepted":outcome.accepted(),"outcome":disposition,"raw_status":raw,"detail":redact(outcome.detail.clone(),endpoint),"reason":outcome.reason.as_ref().map(|reason| message(reason,endpoint))})
}
fn apply_readback(
    record: &mut Json,
    observed: Result<opclook_contract::model::DataValue, String>,
    endpoint: &EndpointSpec,
) -> Result<(), String> {
    let result = observed.and_then(|data| {
        let usable = data.reason.is_none()
            && data.quality.kind == opclook_contract::model::QualityKind::Good;
        let encoded = data_json(&data, endpoint)?;
        record["readback"] =
            json!({"status": if usable { "observed" } else { "unusable" }, "data": encoded});
        if usable {
            Ok(())
        } else {
            Err("read-back returned Bad or Uncertain quality or an error".into())
        }
    });
    if let Err(error) = &result
        && record.get("readback").is_none()
    {
        record["readback"] = json!({"status":"failed", "error":redact(error.clone(), endpoint)});
    }
    if record.get("readback").is_some() && output_to(record, io::sink()).is_err() {
        let error = "read-back JSON output limit exceeded".to_string();
        record["readback"] = json!({"status":"failed", "error":error});
        return Err(error);
    }
    result
}

// Serialize into a bounded buffer before writing, so oversized output never emits a
// partial JSON record and escaped strings count against the actual byte limit.
struct LimitedOutput(Vec<u8>);
impl Write for LimitedOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_JSON_BUDGET.saturating_sub(self.0.len()) {
            return Err(io::Error::other("JSON output limit exceeded"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn output_to(value: &Json, mut writer: impl Write) -> Result<(), String> {
    let mut bytes = LimitedOutput(Vec::new());
    serde_json::to_writer(&mut bytes, value).map_err(|e| e.to_string())?;
    bytes.0.push(b'\n');
    writer.write_all(&bytes.0).map_err(|e| {
        if e.kind() == io::ErrorKind::BrokenPipe {
            OUTPUT_CLOSED.to_string()
        } else {
            e.to_string()
        }
    })
}
fn output(value: &Json) -> Result<(), String> {
    output_to(value, io::stdout().lock())
}

fn parse_write(kind: &str, value: &str) -> Result<Value, String> {
    match kind {
        "bool" => value
            .parse()
            .map(Value::Bool)
            .map_err(|_| "invalid bool".into()),
        "i64" => value
            .parse()
            .map(Value::I64)
            .map_err(|_| "invalid i64".into()),
        "u64" => value
            .parse()
            .map(Value::U64)
            .map_err(|_| "invalid u64".into()),
        "f64" => value
            .parse()
            .map(Value::F64)
            .map_err(|_| "invalid f64".into()),
        "string" => {
            opclook_contract::actions::validate_action_text(value)
                .map_err(|reason| format!("action payload exceeds limits: {reason:?}"))?;
            Ok(Value::Str(value.to_owned()))
        }
        _ => Err("unsupported write type".into()),
    }
}
fn emit_cli(staged: &mut Option<Json>, streaming: bool, record: &Json) -> Result<(), String> {
    if streaming {
        output(record)
    } else {
        *staged = Some(record.clone());
        Ok(())
    }
}

fn append_observations(
    record: &mut Json,
    batch: opclook_contract::read_observer::ReadObservationBatch,
    endpoint: &EndpointSpec,
) {
    if !batch.latest.is_empty() {
        record["read_observations"] = Json::Array(batch.latest.iter().map(|entry| json!({"session_id":entry.session.get(), "epoch":entry.epoch.map(|v| v.get()), "terminal":read_terminal_json(&entry.terminal, endpoint)})).collect());
    }
    if batch.replaced != 0 || batch.overflow != 0 {
        record["read_terminal_notice"] =
            json!({"coalesced":batch.replaced,"overflow":batch.overflow});
    }
}

fn run(options: &Options) -> Result<(), String> {
    let stop = interrupt_receiver()?;
    let mut engine = Engine::<()>::default();
    let observations = std::sync::Arc::new(std::sync::Mutex::new(
        opclook_contract::read_observer::ReadObservations::default(),
    ));
    let sink = observations.clone();
    engine.set_read_terminal_sink(Some(Box::new(move |session, epoch, terminal| {
        use opclook_contract::model::ReadTerminalStatus;
        // Correlated replies are consumed from PendingRequest. The sink owns
        // uncorrelated initial/pending cancellations and dispatch failures.
        if terminal.request_id.is_none()
            || matches!(
                terminal.status,
                ReadTerminalStatus::Cancelled(_) | ReadTerminalStatus::DispatchFailed(_)
            )
        {
            sink.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .record(session, epoch, terminal);
        }
    })));
    let mut staged = None;
    let result = run_with_engine(options, &stop, &mut engine, &observations, &mut staged);
    let shutdown = engine.shutdown();
    let node = options.command.get(1).map(String::as_str).unwrap_or("");
    let batch = observations
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .drain();
    if let Some(mut record) = staged {
        append_observations(&mut record, batch, &options.endpoint);
        output(&record)?;
    } else if !batch.latest.is_empty() || batch.replaced != 0 || batch.overflow != 0 {
        let mut record = json!({"node_id":node});
        append_observations(&mut record, batch, &options.endpoint);
        output(&record)?;
    }
    let errors: Vec<String> = [("DA", shutdown.da), ("UA", shutdown.ua)]
        .into_iter()
        .filter_map(|(protocol, outcome)| {
            opclook_locale::shutdown_error_text(Lang::En, protocol, outcome)
        })
        .collect();
    if errors.is_empty() {
        result
    } else {
        let shutdown_error = errors.join("; ");
        match result {
            Ok(()) => Err(shutdown_error),
            Err(error) => Err(format!("{error}; {shutdown_error}")),
        }
    }
}
fn run_with_engine(
    options: &Options,
    stop: &Receiver<Result<(), String>>,
    mut engine: &mut Engine<()>,
    observations: &std::sync::Arc<
        std::sync::Mutex<opclook_contract::read_observer::ReadObservations>,
    >,
    staged: &mut Option<Json>,
) -> Result<(), String> {
    let protocol = options.endpoint.protocol;
    let command = options
        .command
        .first()
        .map(String::as_str)
        .ok_or("a command is required")?;
    let streaming = command == "watch";
    let expected = match command {
        "browse" => 1..=2,
        "read" | "watch" => 2..=2,
        "write" => 4..=4,
        _ => return Err("unknown command".into()),
    };
    if !expected.contains(&options.command.len()) {
        return Err("wrong number of command arguments".into());
    }
    let id = send(
        &mut engine,
        protocol,
        CommandKind::Connect {
            endpoint: options.endpoint.clone(),
        },
    )?;
    match response(&mut engine, id, options.timeout, stop, &options.endpoint)
        .map_err(|error| format!("connect: {error}"))?
    {
        EventKind::Connected { .. } => {}
        _ => return Err("unexpected connect response".into()),
    }
    if command == "browse" {
        let node_id = options.command.get(1).cloned().unwrap_or_default();
        let id = send(
            &mut engine,
            protocol,
            CommandKind::Browse {
                node_id,
                filter: opclook_contract::model::NodeClassFilter::All,
                view_id: None,
            },
        )?;
        return match response(&mut engine, id, options.timeout, stop, &options.endpoint)
            .map_err(|error| format!("browse: {error}"))?
        {
            EventKind::Browse { node_id, children } => {
                let nodes:Vec<Json>=children.iter().map(|n|json!({"node_id":n.node_id,"name":n.name,"kind":match n.kind { opclook_contract::model::NodeKind::Branch => "branch",opclook_contract::model::NodeKind::Item => "item" },"has_children":n.has_children})).collect();
                emit_cli(
                    staged,
                    streaming,
                    &json!({"node_id":node_id,"children":nodes}),
                )
            }
            _ => Err("unexpected browse response".into()),
        };
    }
    let node = options.command.get(1).ok_or("node is required")?.clone();
    if command == "watch" && protocol == Protocol::OpcUa && !options.poll {
        send(
            &mut engine,
            protocol,
            CommandKind::ConfigureUaSubscription {
                publishing_interval_ms: options.interval.as_millis().min(u64::MAX as u128) as u64,
                deadband: 0.0,
            },
        )?;
    }
    let id = send(
        &mut engine,
        protocol,
        CommandKind::AddItems {
            items: vec![ItemSpec {
                key: ItemKey::FIRST,
                item_id: node.clone(),
            }],
        },
    )?;
    let (add_reply, early_watch) = add_response(
        &mut engine,
        id,
        options.timeout,
        stop,
        &options.endpoint,
        command == "watch",
    )?;
    match add_reply {
        EventKind::AddItems { results } => {
            let [(key, result)] = results.as_slice() else {
                return Err("invalid add response".into());
            };
            if *key != ItemKey::FIRST {
                return Err("invalid add response key".into());
            }
            if let Err(error) = result {
                return Err(message(error, &options.endpoint));
            }
        }
        _ => return Err("unexpected add response".into()),
    }
    if command == "write" {
        let value = parse_write(
            options.command.get(2).ok_or("type required")?,
            options.command.get(3).ok_or("value required")?,
        )?;
        let request = WriteRequest {
            key: ItemKey::FIRST,
            value,
            type_hint: None,
        };
        let id = send(&mut engine, protocol, CommandKind::Write { request })?;

        return match write_response(&mut engine, id, options.timeout, stop) {
            Ok(outcome) => {
                let mut record = write_outcome_json(&node, &outcome, &options.endpoint);
                if !outcome.accepted() {
                    emit_cli(staged, streaming, &record)?;
                    return Err(redact(
                        opclook_locale::write_outcome_text(Lang::En, &outcome),
                        &options.endpoint,
                    ));
                }
                if options.read_after_write {
                    let observed = (|| -> Result<opclook_contract::model::DataValue, String> {
                        let id = send_explicit_due(
                            &mut engine,
                            options.interval,
                            opclook_engine::ReadDemandKind::WriteReadback,
                        )?;
                        let terminal = read_response(
                            &mut engine,
                            id,
                            options.timeout,
                            stop,
                            &options.endpoint,
                        )?;
                        record["read_terminal"] = read_terminal_json(&terminal, &options.endpoint);
                        terminal_data(&terminal, &options.endpoint)
                    })();
                    let readback_result = apply_readback(&mut record, observed, &options.endpoint);
                    emit_cli(staged, streaming, &record)?;
                    readback_result?;
                } else {
                    emit_cli(staged, streaming, &record)?;
                }
                Ok(())
            }
            Err(error) => Err(format!(
                "{}: {}",
                opclook_locale::tr(Lang::En, opclook_locale::Tr::WriteNotSubmitted),
                message(&error, &options.endpoint)
            )),
        };
    }
    if command == "read" {
        let id = if protocol == Protocol::OpcUa {
            engine
                .request_refresh(SessionId::FIRST, ItemKey::FIRST)
                .map_err(|error| format!("refresh unavailable: {error:?}"))?;
            send_explicit_due(
                &mut engine,
                options.interval,
                opclook_engine::ReadDemandKind::ExplicitRefresh,
            )?
        } else {
            send(
                &mut engine,
                protocol,
                CommandKind::Read {
                    keys: vec![ItemKey::FIRST],
                },
            )?
        };
        let terminal = read_response(&mut engine, id, options.timeout, stop, &options.endpoint)?;
        let summary = read_terminal_json(&terminal, &options.endpoint);
        match terminal_data(&terminal, &options.endpoint) {
            Ok(data) => {
                emit_cli(
                    staged,
                    streaming,
                    &json!({"node_id":node,"data":data_json(&data,&options.endpoint)?, "read_terminal":summary}),
                )?;
                if let Some(reason) = data.reason {
                    Err(message(&reason, &options.endpoint))
                } else if data.quality.kind != QualityKind::Good {
                    Err("read returned Bad or Uncertain quality".into())
                } else {
                    Ok(())
                }
            }
            Err(error) => {
                emit_cli(
                    staged,
                    streaming,
                    &json!({"node_id":node,"read_terminal":summary}),
                )?;
                Err(error)
            }
        }?;
        return Ok(());
    }
    let mut count = 0usize;
    if let Some(data) = early_watch {
        let data = data?;
        let mut record = json!({"node_id":node,"data":data_json(&data, &options.endpoint)?});
        let batch = observations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .drain();
        append_observations(&mut record, batch, &options.endpoint);
        emit_cli(staged, streaming, &record)?;
        count += 1;
        if let Some(reason) = data.reason {
            return Err(message(&reason, &options.endpoint));
        }
    }
    let mut event_batch = 0usize;
    while options.count.is_none_or(|limit| count < limit) {
        if event_batch >= 64 {
            dispatch_due(&mut engine, options.interval)?;
            event_batch = 0;
        }
        if stopped(stop)? {
            break;
        }
        let now = Instant::now();
        if let Some(request) = engine.expired_read_request(SessionId::FIRST, now, options.timeout) {
            engine.timeout_read_request(SessionId::FIRST, request, options.timeout.as_secs());
        } else if engine.has_expired_operation(
            SessionId::FIRST,
            now,
            options.timeout,
            &[Operation::Test],
        ) {
            return Err("operation timed out".into());
        }
        let Some(event) = engine.try_recv() else {
            dispatch_due(&mut engine, options.interval)?;
            event_batch = 0;
            std::thread::sleep(Duration::from_millis(10));
            continue;
        };
        event_batch += 1;
        let accepted = match engine.accept(&event) {
            Ok(pending) => pending,
            Err(()) => continue,
        };
        let is_read_reply = accepted
            .as_ref()
            .is_some_and(|pending| pending.operation == Operation::Read);
        let mut terminals = accepted.map(|p| p.read_terminals).unwrap_or_default();
        match event.kind {
            EventKind::Values { values } => {
                for (key, data) in values {
                    if key == ItemKey::FIRST {
                        let terminal = terminals
                            .iter()
                            .position(|t| t.key == key)
                            .map(|index| terminals.remove(index));
                        if is_read_reply
                            && !terminal.as_ref().is_some_and(|t| {
                                matches!(
                                    t.status,
                                    opclook_contract::model::ReadTerminalStatus::Received(_)
                                )
                            })
                        {
                            if let Some(terminal) = terminal {
                                output(
                                    &json!({"node_id":node,"read_terminal":read_terminal_json(&terminal, &options.endpoint)}),
                                )?;
                            }
                            continue;
                        }
                        let mut record =
                            json!({"node_id":node,"data":data_json(&data,&options.endpoint)?});
                        if let Some(terminal) = terminal {
                            record["read_terminal"] =
                                read_terminal_json(&terminal, &options.endpoint);
                        }
                        let batch = observations
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .drain();
                        append_observations(&mut record, batch, &options.endpoint);
                        emit_cli(staged, streaming, &record)?;
                        count += 1;
                        if let Some(reason) = data.reason {
                            return Err(message(&reason, &options.endpoint));
                        }
                    }
                }
                for terminal in terminals {
                    emit_cli(
                        staged,
                        streaming,
                        &json!({"node_id":node,"read_terminal":read_terminal_json(&terminal, &options.endpoint)}),
                    )?;
                }
            }
            EventKind::Error { message: error, .. } | EventKind::Disconnected { reason: error } => {
                for terminal in terminals {
                    emit_cli(
                        staged,
                        streaming,
                        &json!({"node_id":node,"read_terminal":read_terminal_json(&terminal, &options.endpoint)}),
                    )?;
                }
                return Err(message(&error, &options.endpoint));
            }
            _ => {}
        }
    }
    Ok(())
}
fn main() {
    match parse(std::env::args().skip(1).collect()) {
        Ok(None) => {
            if let Err(error) = write!(io::stdout().lock(), "{HELP}")
                && error.kind() != io::ErrorKind::BrokenPipe
            {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Ok(Some(options)) => {
            if let Err(error) = run(&options) {
                if error == OUTPUT_CLOSED {
                    return;
                }
                eprintln!("{}", redact(error, &options.endpoint));
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn write_text_budget_is_checked_before_copy_and_keeps_type_errors() {
        use super::*;
        let too_large = "x".repeat(opclook_contract::actions::ACTION_MAX_BYTES + 1);
        assert_eq!(
            parse_write("unknown", &too_large).unwrap_err(),
            "unsupported write type"
        );
        assert!(
            parse_write("string", &too_large)
                .unwrap_err()
                .contains("Bytes")
        );
        let boundary = "я".repeat(opclook_contract::actions::ACTION_MAX_BYTES / 2);
        assert!(
            matches!(parse_write("string", &boundary), Ok(Value::Str(value)) if value == boundary)
        );
    }
    #[test]
    fn json_preserves_separate_time_facts_and_origin() {
        use super::*;
        use opclook_contract::model::{SourceClockId, ValueOrigin, ValueTimes};
        let times = ValueTimes {
            origin: ValueOrigin::Subscription,
            source_clock: Some(SourceClockId(42)),
            ..ValueTimes::default()
        };
        let json = value_times_json(&times);
        assert!(json["ua_source"].is_null());
        assert!(json["ua_server"].is_null());
        assert!(json["da_timestamp"].is_null());
        assert!(json["received_at"].is_null());
        assert_eq!(json["origin"], "subscription");
        assert_eq!(json["source_clock"], 42);
    }
    #[test]
    fn terminal_json_preserves_unusable_quality_and_redacts_reason() {
        use super::*;
        use opclook_contract::model::{Quality, ReadNeed, ReadTerminal, ReadTerminalStatus};
        let mut endpoint = EndpointSpec::ua("opc.tcp://localhost:4840");
        endpoint.password = "secret".into();
        let terminal = ReadTerminal {
            request_id: None,
            key: ItemKey::FIRST,
            needs: vec![ReadNeed::Initial],
            status: ReadTerminalStatus::InitialPush {
                quality: Quality::from_ua(0x4000_0000, "uncertain"),
                timestamp: None,
                times: Default::default(),
                reason: Some(BackendMsg::Raw("secret failure".into())),
            },
        };
        let json = read_terminal_json(&terminal, &endpoint);
        assert_eq!(json["status"], "unusable");
        assert_eq!(json["quality"]["kind"], "uncertain");
        assert!(!json.to_string().contains("secret"));
    }

    #[test]
    fn one_shot_json_stages_one_record_with_observation_counts() {
        use super::*;
        use opclook_contract::model::{ReadNeed, ReadTerminal, ReadTerminalStatus};
        let endpoint = EndpointSpec::ua("opc.tcp://localhost:4840");
        let terminal = ReadTerminal {
            request_id: None,
            key: ItemKey::FIRST,
            needs: vec![ReadNeed::Initial],
            status: ReadTerminalStatus::MissingResult,
        };
        let mut buffer = opclook_contract::read_observer::ReadObservations::default();
        buffer.record(SessionId::FIRST, None, &terminal);
        buffer.record(SessionId::FIRST, None, &terminal);
        let mut staged = None;
        emit_cli(
            &mut staged,
            false,
            &json!({"node_id":"tag", "data": {"value": 1}}),
        )
        .unwrap();
        let mut record = staged.unwrap();
        append_observations(&mut record, buffer.drain(), &endpoint);
        assert_eq!(record["data"]["value"], 1);
        assert_eq!(
            record["read_observations"][0]["terminal"]["status"],
            "missing_result"
        );
        assert_eq!(record["read_terminal_notice"]["coalesced"], 1);
        let mut bytes = Vec::new();
        output_to(&record, &mut bytes).unwrap();
        assert!(serde_json::from_slice::<Json>(&bytes).is_ok());
    }

    #[test]
    fn browse_reports_unavailable_session_before_dispatch() {
        let mut engine = opclook_engine::Engine::<()>::default();
        let result = super::send(
            &mut engine,
            opclook_contract::model::Protocol::OpcUa,
            opclook_contract::model::CommandKind::Browse {
                node_id: String::new(),
                filter: opclook_contract::model::NodeClassFilter::All,
                view_id: None,
            },
        );
        assert_eq!(result.unwrap_err(), "Session not found");
    }
    #[test]
    fn browse_reports_pending_read_before_dispatch() {
        use opclook_contract::model::{
            CommandKind, EndpointSpec, ItemKey, NodeClassFilter, Protocol, SessionId,
        };
        let mut engine = opclook_engine::Engine::<()>::default();
        let mut session = opclook_engine::SessionRuntime::new(
            EndpointSpec::ua("opc.tcp://localhost:4840"),
            std::time::Instant::now(),
        );
        session.state = opclook_engine::SessionState::Connected;
        session.items.insert(ItemKey::FIRST, false);
        engine.seed_session_for_test(SessionId::FIRST, session);
        engine
            .request_refresh(SessionId::FIRST, ItemKey::FIRST)
            .unwrap();
        let error = super::send(
            &mut engine,
            Protocol::OpcUa,
            CommandKind::Browse {
                node_id: String::new(),
                filter: NodeClassFilter::All,
                view_id: None,
            },
        )
        .unwrap_err();
        assert_eq!(error, "A read is pending for this session");
        assert!(engine.pending_ids_for_test(SessionId::FIRST).is_empty());
    }
    #[test]
    fn unavailable_write_diagnostic_does_not_expose_value() {
        let mut engine = opclook_engine::Engine::<()>::default();
        let secret = "write-secret-canary";
        let error = super::send(
            &mut engine,
            opclook_contract::model::Protocol::OpcUa,
            opclook_contract::model::CommandKind::Write {
                request: opclook_contract::model::WriteRequest {
                    key: opclook_contract::model::ItemKey::FIRST,
                    value: opclook_contract::model::Value::Str(secret.into()),
                    type_hint: None,
                },
            },
        )
        .unwrap_err();
        assert!(!error.contains(secret));
        assert_eq!(error, "Session not found");
    }
    #[test]
    fn interface_index_and_scope_are_nonzero_at_argument_boundary() {
        let args = [
            "--ua",
            "opc.tcp://[fe80::2]:4840",
            "--bind",
            "fe80::1",
            "--if-index",
            "12",
            "--scope",
            "7",
            "browse",
        ];
        let options = super::parse(args.iter().map(|s| s.to_string()).collect())
            .unwrap()
            .unwrap();
        let binding = options.endpoint.ua_bind.unwrap();
        assert_eq!(binding.ipv6_if_index, std::num::NonZeroU32::new(12));
        assert_eq!(binding.scope_id, std::num::NonZeroU32::new(7));
        for option in ["--if-index", "--scope"] {
            let args = [
                "--ua",
                "opc.tcp://localhost:4840",
                "--bind",
                "127.0.0.1",
                option,
                "0",
                "browse",
            ];
            assert!(super::parse(args.iter().map(|s| s.to_string()).collect()).is_err());
        }
    }

    use super::*;
    #[test]
    fn redaction_handles_overlapping_credentials_longest_first() {
        let mut endpoint = EndpointSpec::ua("opc.tcp://localhost:4840");
        endpoint.user = "secret".into();
        endpoint.password = "secret-password".into();
        let redacted = redact("secret-password secret".into(), &endpoint);
        assert_eq!(redacted, "[redacted] [redacted]");
        assert!(!redacted.contains("password"));
    }
    #[test]
    fn output_limit_counts_json_escapes_and_never_writes_partial_record() {
        let mut bytes = Vec::new();
        let value = json!("\n".repeat(MAX_JSON_BUDGET / 2));
        assert!(output_to(&value, &mut bytes).is_err());
        assert!(bytes.is_empty());
    }
    #[test]
    fn closed_stdout_is_distinguished_from_other_io_errors() {
        struct Closed;
        impl Write for Closed {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::from(io::ErrorKind::BrokenPipe))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        assert_eq!(output_to(&json!({}), Closed).unwrap_err(), OUTPUT_CLOSED);
    }
    #[test]
    fn oversized_timeout_does_not_panic() {
        assert!(checked_deadline(Instant::now(), Duration::MAX).is_err());
        assert!(checked_deadline(Instant::now(), Duration::from_secs(1)).is_ok());
    }
    #[test]
    fn help_in_write_value_is_not_an_option() {
        let args = [
            "--ua",
            "opc.tcp://127.0.0.1:4840",
            "write",
            "i=1",
            "string",
            "--help",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        assert_eq!(
            parse(args)
                .unwrap()
                .unwrap()
                .command
                .last()
                .map(String::as_str),
            Some("--help")
        );
    }
    #[test]
    fn json_preserves_integer_width_and_nonfinite_floats() {
        let mut budget = 100;
        assert_eq!(
            encode(&Value::U64(u64::MAX), 0, &mut budget).unwrap()["value"],
            u64::MAX.to_string()
        );
        assert_eq!(
            encode(&Value::F64(f64::NAN), 0, &mut budget).unwrap()["value"],
            "NaN"
        );
        assert_eq!(
            encode(&Value::F64(f64::INFINITY), 0, &mut budget).unwrap()["value"],
            "Infinity"
        );
    }

    #[test]
    fn explicit_cli_poll_alias_uses_same_profile_and_rejects_invalid_options() {
        fn args(words: &[&str]) -> Vec<String> {
            words.iter().map(|s| s.to_string()).collect()
        }
        let alias = parse(args(&[
            "--ua",
            "opc.tcp://localhost:4840",
            "--poll",
            "watch",
            "i=1",
        ]))
        .unwrap()
        .unwrap();
        let explicit = parse(args(&[
            "--ua",
            "opc.tcp://localhost:4840",
            "--update-mode",
            "poll",
            "watch",
            "i=1",
        ]))
        .unwrap()
        .unwrap();
        assert_eq!(alias.endpoint.acquisition, explicit.endpoint.acquisition);
        assert!(!alias.read_after_write);
        let enabled = parse(args(&[
            "--ua",
            "opc.tcp://localhost:4840",
            "--read-after-write",
            "true",
            "write",
            "i=1",
            "i64",
            "2",
        ]))
        .unwrap()
        .unwrap();
        assert!(enabled.read_after_write);
        for bad in [
            vec!["--interval", "1", "--interval-ms", "1000"],
            vec!["--interval-ms", "0"],
            vec!["--interval-ms", "3600001"],
            vec!["--update-mode", "unknown"],
            vec!["--poll-freshness", "other"],
            vec!["--read-after-write", "truue"],
        ] {
            let mut all = vec!["--ua", "opc.tcp://localhost:4840"];
            all.extend(bad);
            all.extend(["watch", "i=1"]);
            assert!(parse(args(&all)).is_err());
        }
        assert!(
            parse(args(&[
                "--da",
                "",
                "server",
                "--read-after-write",
                "true",
                "write",
                "x",
                "i64",
                "1"
            ]))
            .is_err()
        );
    }

    #[test]
    fn wrong_key_write_reply_is_unknown_and_initial_read_is_not_readback() {
        use opclook_contract::model::{
            ItemInfo, Origin, Owner, SubscriptionState, WriteDisposition, WriteOutcome,
        };
        use opclook_engine::{ReadDemandKind, SessionRuntime, SessionState};
        let mut engine = Engine::<()>::default();
        engine.limit_dispatch_for_test(usize::MAX);
        let mut view =
            SessionRuntime::new(EndpointSpec::ua("opc.tcp://localhost:4840"), Instant::now());
        view.state = SessionState::Connected;
        view.subscription = SubscriptionState::Active;
        engine.seed_session_for_test(SessionId::FIRST, view);
        let add = engine
            .send(
                SessionId::FIRST,
                Protocol::OpcUa,
                CommandKind::AddItems {
                    items: vec![ItemSpec {
                        key: ItemKey::FIRST,
                        item_id: "a".into(),
                    }],
                },
                (),
            )
            .unwrap();
        engine
            .accept(&Event {
                owner: Owner::Session(SessionId::FIRST),
                origin: Origin::Reply(add),
                kind: EventKind::AddItems {
                    results: vec![(
                        ItemKey::FIRST,
                        Ok(ItemInfo {
                            data_type: None,
                            type_id: None,
                            writable: true,
                        }),
                    )],
                },
            })
            .unwrap();
        let id = engine
            .send_write_with_readback(
                SessionId::FIRST,
                Protocol::OpcUa,
                WriteRequest {
                    key: ItemKey::FIRST,
                    value: Value::I64(1),
                    type_hint: None,
                },
                (),
            )
            .unwrap();
        engine.inject_event_for_test(Event {
            owner: Owner::Session(SessionId::FIRST),
            origin: Origin::Reply(id),
            kind: EventKind::Wrote {
                key: ItemKey::new(2).unwrap(),
                result: Ok(WriteOutcome::from_ua(0, "good")),
            },
        });
        let (_tx, rx) = mpsc::channel();
        let outcome = write_response(&mut engine, id, Duration::from_secs(5), &rx).unwrap();
        assert_eq!(outcome.disposition, WriteDisposition::Unknown);
        assert!(
            send_explicit_due(&mut engine, Duration::ZERO, ReadDemandKind::WriteReadback).is_err()
        );
        let sent = engine.dispatched_for_test();
        assert_eq!(
            sent.iter()
                .filter(|cmd| matches!(cmd.kind, CommandKind::Write { .. }))
                .count(),
            1
        );
        assert_eq!(
            sent.iter()
                .filter(|cmd| matches!(cmd.kind, CommandKind::Read { .. }))
                .count(),
            0
        );
    }

    #[test]
    fn readback_failure_and_quality_preserve_accepted_write_record() {
        use opclook_contract::model::{DataValue, Quality, WriteOutcome};
        let endpoint = EndpointSpec::ua("opc.tcp://localhost:4840");
        for kind in [
            "missing",
            "transport",
            "bad",
            "uncertain",
            "oversize",
            "escaped",
            "different",
        ] {
            let mut record = write_outcome_json("a", &WriteOutcome::from_ua(0, "good"), &endpoint);
            let result = match kind {
                "missing" => Err("read-back returned no value".into()),
                "transport" => Err("read-back timeout".into()),
                _ => Ok(DataValue {
                    value: if kind == "oversize" {
                        Value::Str("x".repeat(MAX_JSON_BUDGET + 1))
                    } else if kind == "escaped" {
                        Value::Str("\u{0001}".repeat(200_000))
                    } else {
                        Value::I64(9)
                    },
                    quality: Quality::from_ua(
                        match kind {
                            "bad" => 0x8000_0000,
                            "uncertain" => 0x4000_0000,
                            _ => 0,
                        },
                        "status",
                    ),
                    timestamp: None,
                    reason: None,
                    times: Default::default(),
                }),
            };
            let applied = apply_readback(&mut record, result, &endpoint);
            assert_eq!(record["accepted"], true);
            assert_eq!(record["outcome"], "accepted");
            assert!(output_to(&record, io::sink()).is_ok());
            assert_eq!(applied.is_ok(), kind == "different");
            assert_eq!(
                record["readback"]["status"],
                match kind {
                    "bad" | "uncertain" => "unusable",
                    "different" => "observed",
                    _ => "failed",
                }
            );
        }
    }

    #[test]
    fn write_json_distinguishes_uncertain_async_and_service_status() {
        use opclook_contract::model::{NativeWriteStatus, WriteOutcome};
        let endpoint = EndpointSpec::ua("opc.tcp://localhost:4840");
        let uncertain = write_outcome_json(
            "a",
            &WriteOutcome::from_ua(0x4000_1234, "uncertain"),
            &endpoint,
        );
        assert_eq!(uncertain["accepted"], false);
        assert_eq!(uncertain["outcome"], "uncertain");
        assert_eq!(uncertain["raw_status"]["code"], 0x4000_1234u32);
        let async_result =
            write_outcome_json("a", &WriteOutcome::from_ua(0x002e_0000, "async"), &endpoint);
        assert_eq!(async_result["outcome"], "accepted_asynchronously");
        let unknown = write_outcome_json(
            "a",
            &WriteOutcome::unknown_with_status(NativeWriteStatus::UaService(0x8000_0000), "error"),
            &endpoint,
        );
        assert_eq!(unknown["accepted"], false);
        assert_eq!(unknown["raw_status"]["scope"], "service");
    }
}
