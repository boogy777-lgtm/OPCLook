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

const HELP: &str = "opclook-cli — headless OPC DA/UA client\n\nUsage: opclook-cli [options] COMMAND [arguments]\n\nEndpoints (choose one):\n  --ua URL                  OPC UA endpoint\n  --da HOST PROGID          OPC DA server (Windows)\n\nOptions:\n  --timeout SECONDS         Per-operation deadline (default 30)\n  --bind IP                 Local UA source address\n  --if-index INDEX          Interface index for the source address\n  --scope INDEX             IPv6 scope ID\n  --policy POLICY           UA policy (Auto, None, Basic256Sha256, etc.)\n  --mode MODE               UA mode (Auto, None, Sign, SignAndEncrypt)\n  --trust-server-cert       Trust the UA server certificate\n  --client-cert             Use the client certificate from the PKI store\n  --session-timeout-ms N    Requested UA session timeout in milliseconds\n  --user-env NAME           Read username from an environment variable\n  --password-env NAME       Read password from an environment variable\n  --interval SECONDS        Watch polling interval (default 1)\n  --count N                 Stop watch after N value records (default: Ctrl+C)\n  --poll                    Watch by periodic reads instead of UA subscription\n  --json                    Emit JSON or JSONL (the default)\n  --help                    Print this help\n\nCommands:\n  browse [NODE]             Browse a node (default: root)\n  read NODE                 Add and read a node\n  write NODE TYPE VALUE     Write bool, i64, u64, f64 or string\n  watch NODE                Subscribe/poll until count or Ctrl+C\n\n64-bit integers are decimal strings. Non-finite f64 values are strings NaN,\nInfinity and -Infinity. Credentials are read from the environment.\n";
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
            "--poll" => poll = true,
            "--help" | "-h" => return Ok(None),
            _ => return Err("unknown option".into()),
        }
        i += 1;
    }
    let mut endpoint = endpoint.ok_or("an endpoint is required")?;
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
    dispatch.map_err(|error| match error {
        DispatchError::Exhausted => "request identifiers exhausted".into(),
        DispatchError::SessionLimit => "UA session limit reached".into(),
        DispatchError::ActionUnavailable(reason) => {
            let key = match reason {
                opclook_engine::ActionUnavailable::RequestIdsExhausted => {
                    opclook_locale::Tr::RequestIdsExhausted
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
    })
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
fn next_event(
    engine: &mut Engine<()>,
    deadline: Instant,
    stop: &Receiver<Result<(), String>>,
) -> Result<Event, String> {
    loop {
        if stopped(stop)? {
            return Err("interrupted".into());
        }
        if Instant::now() >= deadline {
            return Err("operation timed out".into());
        }
        if let Some(event) = engine.try_recv() {
            if engine.accept(&event).is_ok() {
                return Ok(event);
            }
        } else {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
fn response(
    engine: &mut Engine<()>,
    request: opclook_contract::model::RequestId,
    timeout: Duration,
    stop: &Receiver<Result<(), String>>,
    endpoint: &EndpointSpec,
) -> Result<EventKind, String> {
    let deadline = Instant::now() + timeout;
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
        json!({"value":encode(&data.value,0,&mut MAX_JSON_BUDGET.clone())?,"quality":{"code":data.quality.code,"kind":quality,"text":data.quality.text},"timestamp":data.timestamp.map(|v| v.to_rfc3339()),"reason":data.reason.as_ref().map(|reason| message(reason,endpoint))}),
    )
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
        "string" => Ok(Value::Str(value.to_owned())),
        _ => Err("unsupported write type".into()),
    }
}
fn run(options: &Options) -> Result<(), String> {
    let stop = interrupt_receiver()?;
    let mut engine = Engine::<()>::default();
    let result = run_with_engine(options, &stop, &mut engine);
    let shutdown = engine.shutdown();
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
) -> Result<(), String> {
    let protocol = options.endpoint.protocol;
    let command = options
        .command
        .first()
        .map(String::as_str)
        .ok_or("a command is required")?;
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
    match response(&mut engine, id, options.timeout, stop, &options.endpoint)? {
        EventKind::Connected { .. } => {}
        _ => return Err("unexpected connect response".into()),
    }
    if options.poll {
        engine.set_force_poll(SessionId::FIRST, true);
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
        return match response(&mut engine, id, options.timeout, stop, &options.endpoint)? {
            EventKind::Browse { node_id, children } => {
                let nodes:Vec<Json>=children.iter().map(|n|json!({"node_id":n.node_id,"name":n.name,"kind":match n.kind { opclook_contract::model::NodeKind::Branch => "branch",opclook_contract::model::NodeKind::Item => "item" },"has_children":n.has_children})).collect();
                output(&json!({"node_id":node_id,"children":nodes}))
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
    match response(&mut engine, id, options.timeout, stop, &options.endpoint)? {
        EventKind::AddItems { results } => {
            for (_, result) in results {
                if let Err(error) = result {
                    return Err(message(&error, &options.endpoint));
                }
            }
        }
        _ => return Err("unexpected add response".into()),
    }
    if command == "write" {
        let value = parse_write(
            options.command.get(2).ok_or("type required")?,
            options.command.get(3).ok_or("value required")?,
        )?;
        let id = send(
            &mut engine,
            protocol,
            CommandKind::Write {
                request: WriteRequest {
                    key: ItemKey::FIRST,
                    value,
                    type_hint: None,
                },
            },
        )?;
        return match response(&mut engine, id, options.timeout, stop, &options.endpoint)? {
            EventKind::Wrote { result: Ok(()), .. } => {
                output(&json!({"node_id":node,"written":true}))
            }
            EventKind::Wrote {
                result: Err(error), ..
            } => Err(message(&error, &options.endpoint)),
            _ => Err("unexpected write response".into()),
        };
    }
    if command == "read" {
        let id = send(
            &mut engine,
            protocol,
            CommandKind::Read {
                keys: vec![ItemKey::FIRST],
            },
        )?;
        return match response(&mut engine, id, options.timeout, stop, &options.endpoint)? {
            EventKind::Values { values } => {
                let data = values
                    .into_iter()
                    .find_map(|(key, data)| (key == ItemKey::FIRST).then_some(data))
                    .ok_or("server returned no value")?;
                output(&json!({"node_id":node,"data":data_json(&data,&options.endpoint)?}))?;
                if let Some(reason) = data.reason {
                    Err(message(&reason, &options.endpoint))
                } else {
                    Ok(())
                }
            }
            _ => Err("unexpected read response".into()),
        };
    }
    let mut count = 0usize;
    while options.count.is_none_or(|limit| count < limit) {
        for due in engine.due(
            Instant::now(),
            options.interval,
            Some(Duration::from_secs(30)),
        ) {
            match due {
                Due::Read {
                    session,
                    protocol,
                    keys,
                } if session == SessionId::FIRST => {
                    send(&mut engine, protocol, CommandKind::Read { keys })?;
                }
                Due::Test { session, protocol } if session == SessionId::FIRST => {
                    send(&mut engine, protocol, CommandKind::Test)?;
                }
                _ => {}
            }
        }
        if stopped(stop)? {
            break;
        }
        if engine.has_expired_operation(
            SessionId::FIRST,
            Instant::now(),
            options.timeout,
            &[Operation::Read, Operation::Test],
        ) {
            return Err("operation timed out".into());
        }
        let Some(event) = engine.try_recv() else {
            std::thread::sleep(Duration::from_millis(10));
            continue;
        };
        if engine.accept(&event).is_err() {
            continue;
        }
        if discard_poll_push(options.poll, &event) {
            continue;
        }
        match event.kind {
            EventKind::Values { values } => {
                for (key, data) in values {
                    if key == ItemKey::FIRST {
                        output(
                            &json!({"node_id":node,"data":data_json(&data,&options.endpoint)?}),
                        )?;
                        count += 1;
                        if let Some(reason) = data.reason {
                            return Err(message(&reason, &options.endpoint));
                        }
                    }
                }
            }
            EventKind::Error { message: error, .. } | EventKind::Disconnected { reason: error } => {
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
fn discard_poll_push(poll: bool, event: &Event) -> bool {
    poll && matches!(
        event.origin,
        opclook_contract::model::Origin::Unsolicited | opclook_contract::model::Origin::Session(_)
    ) && matches!(event.kind, EventKind::Values { .. })
}
#[cfg(test)]
mod tests {
    #[test]
    fn poll_discards_both_push_origins_but_keeps_read_reply() {
        use opclook_contract::model::{Event, EventKind, Origin, Owner, RequestId, SessionId};
        for (origin, discard) in [
            (Origin::Unsolicited, true),
            (Origin::Session(RequestId::FIRST), true),
            (Origin::Reply(RequestId::FIRST), false),
        ] {
            let event = Event {
                owner: Owner::Session(SessionId::FIRST),
                origin,
                kind: EventKind::Values { values: vec![] },
            };
            assert_eq!(super::discard_poll_push(true, &event), discard);
            assert!(!super::discard_poll_push(false, &event));
        }
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
}
