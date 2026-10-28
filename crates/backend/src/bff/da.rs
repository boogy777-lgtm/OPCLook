#![deny(clippy::undocumented_unsafe_blocks)]

use super::com::{CoAllocation, CoString, ComArray, OwnedVariant};
use super::da_clsid::{ClsidLookup, DiscoveredClsids, is_local_host, resolve_clsid};
use super::da_router::{DaHost, SESSION_JOIN_BUDGET, run_router};
use super::da_status::{ServerStatusInfo, check_server_state, format_connect_info};
use super::da_variant::{
    clear_variant, filetime_to_utc, variant_from_value, variant_to_value, vt_name,
};
use super::model::{
    BackendMsg, BrowseNode, Command, CommandKind, DataValue, Event, EventKind, ItemInfo, ItemKey,
    ItemSpec, NodeKind, Operation, Quality, ServerEntry, SessionId,
};
use crate::opc::bindings::comn::{IOPCServerList, IOPCServerList2};
use crate::opc::bindings::da::{
    IOPCBrowseServerAddressSpace, IOPCItemMgt, IOPCServer, IOPCSyncIO, OPC_BRANCH, OPC_BROWSE_DOWN,
    OPC_BROWSE_TO, OPC_DS_CACHE, OPC_LEAF, OPC_NS_FLAT, OPC_WRITEABLE, tagOPCITEMDEF,
    tagOPCITEMRESULT, tagOPCITEMSTATE, tagOPCNAMESPACETYPE,
};
use std::collections::HashMap;
use std::mem::ManuallyDrop;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock, PoisonError};
use tokio::sync::mpsc::UnboundedReceiver;
use windows::Win32::System::Com::{
    CLSCTX_LOCAL_SERVER, CLSCTX_REMOTE_SERVER, CLSCTX_SERVER, CLSIDFromProgID,
    COINIT_MULTITHREADED, COSERVERINFO, CoCreateInstance, CoCreateInstanceEx, CoInitializeEx,
    CoInitializeSecurity, CoUninitialize, EOAC_NONE, MULTI_QI, RPC_C_AUTHN_LEVEL_NONE,
    RPC_C_IMP_LEVEL_IDENTIFY,
};
use windows::Win32::System::Variant::VARIANT;
use windows::core::{GUID, HRESULT, IUnknown, Interface, PCWSTR, PWSTR};

const CLSID_SERVER_LIST: GUID = GUID::from_u128(0x13486D51_4821_11D2_A494_3CB306C10000);
const MAX_BROWSE_NODES: usize = 5000;
const GROUP_UPDATE_RATE_MS: u32 = 1000;

struct ComGuard {
    initialized: bool,
}

impl ComGuard {
    fn new() -> Self {
        // SAFETY: initialize COM for the current worker thread with no reserved pointer.
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        Self {
            initialized: hr.is_ok(),
        }
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.initialized {
            // SAFETY: this thread successfully initialized COM and balances it once.
            unsafe { CoUninitialize() };
        }
    }
}

struct ItemSlot {
    server_handle: u32,
}

struct DaSession {
    endpoint: super::model::EndpointSpec,
    server: IOPCServer,
    browse: Option<IOPCBrowseServerAddressSpace>,
    item_mgt: IOPCItemMgt,
    sync: IOPCSyncIO,
    group: ServerGroup,
    items: HashMap<ItemKey, ItemSlot>,
    info: BackendMsg,
}

/// The COM side of the router: each session and each discovery runs on its
/// own thread with its own COM apartment, so a hung DCOM call blocks only
/// that thread (D2).
struct WinDaHost;

impl DaHost for WinDaHost {
    fn run_session(
        &self,
        _session: SessionId,
        commands: &mut UnboundedReceiver<Command>,
        events: &Sender<Event>,
    ) {
        let _com = ComGuard::new();
        let mut sessions: HashMap<SessionId, DaSession> = HashMap::new();
        while let Some(command) = commands.blocking_recv() {
            let disconnect = matches!(command.kind, CommandKind::Disconnect);
            handle_command(command, &mut sessions, events);
            // Over after `Disconnect`, or after a failed `Connect` (no session
            // was stored); the router answers whatever is still queued.
            if disconnect || sessions.is_empty() {
                break;
            }
        }
        // Channel closed (gateway shutdown) or session over: drop the group.
        for (_, session) in sessions {
            session.shutdown();
        }
    }

    fn run_discovery(&self, command: Command, events: &Sender<Event>) {
        let _com = ComGuard::new();
        handle_command(command, &mut HashMap::new(), events);
    }
}

/// `CoInitializeSecurity` with no authentication and `IDENTIFY` impersonation,
/// as OPC clients do: without it remote DCOM calls fail with `E_ACCESSDENIED`
/// or `RPC_S_SERVER_UNAVAILABLE` (D3). It may be called once per process and
/// fails with `RPC_E_TOO_LATE` when the process already configured COM
/// security; that is reported in the journal and is not fatal.
fn init_dcom_security() -> Result<(), String> {
    // SAFETY: a null security descriptor and null reserved/list pointers are
    // the documented way to ask COM for the default security settings.
    unsafe {
        CoInitializeSecurity(
            None,
            -1,
            None,
            None,
            RPC_C_AUTHN_LEVEL_NONE,
            RPC_C_IMP_LEVEL_IDENTIFY,
            None,
            EOAC_NONE,
            None,
        )
    }
    .map_err(|e| format!("{e}"))
}

/// Run [`init_dcom_security`] once per process. Only the call that actually
/// performed the initialization returns its error; every later gateway (tests
/// and the UI create several in one process) returns `None` instead of an
/// `RPC_E_TOO_LATE` journal line of its own — CoInitializeSecurity is
/// process-wide, so there is nothing more to report.
fn init_dcom_security_once() -> Option<String> {
    static ONCE: std::sync::Once = std::sync::Once::new();
    let mut outcome = None;
    ONCE.call_once(|| {
        if let Err(error) = init_dcom_security() {
            outcome = Some(error);
        }
    });
    outcome
}

pub(crate) fn spawn(event_tx: Sender<Event>) -> super::WorkerHandle {
    let (cmd_tx, cmd_rx) = tokio::sync::mpsc::unbounded_channel();
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    let join = std::thread::Builder::new()
        .name("opc-da".to_string())
        .spawn(move || {
            // Keep the router thread in the MTA for as long as it runs, so the
            // process-wide COM security settings stay in force.
            let _com = ComGuard::new();
            if let Some(e) = init_dcom_security_once() {
                send(
                    &event_tx,
                    Event {
                        request_id: 0,
                        session: 0,
                        kind: EventKind::Log {
                            message: BackendMsg::DaDcomSecurityNotSet { detail: e },
                        },
                    },
                );
            }
            run_router(Arc::new(WinDaHost), cmd_rx, event_tx, SESSION_JOIN_BUDGET);
            let _ = done_tx.send(());
        });
    match join {
        Ok(join) => super::WorkerHandle::new(cmd_tx, join, done_rx),
        // The thread could not be created: hand out a handle with a closed
        // command channel so `Gateway::send` reports the failure to the caller.
        Err(_) => super::WorkerHandle::closed(done_rx),
    }
}

fn handle_command(
    command: Command,
    sessions: &mut HashMap<SessionId, DaSession>,
    tx: &Sender<Event>,
) {
    let session_id = command.session;
    let request_id = command.request_id;
    match command.kind {
        CommandKind::Discover { host } => match enumerate_servers(&host) {
            Ok(servers) => send(
                tx,
                Event {
                    request_id,
                    session: session_id,
                    kind: EventKind::Servers { host, servers },
                },
            ),
            Err(message) => send(
                tx,
                Event {
                    request_id,
                    session: session_id,
                    kind: EventKind::Error {
                        operation: Operation::Discover,
                        message,
                    },
                },
            ),
        },
        CommandKind::SearchUaServers { .. } => send_error(
            tx,
            request_id,
            session_id,
            Operation::SearchServers,
            BackendMsg::DaNoUaServerSearch,
        ),
        CommandKind::SearchUaNetwork { .. } => send_error(
            tx,
            request_id,
            session_id,
            Operation::SearchNetwork,
            BackendMsg::DaNoUaNetworkSearch,
        ),
        CommandKind::Connect { endpoint } => {
            if sessions.contains_key(&session_id) {
                send_error(
                    tx,
                    request_id,
                    session_id,
                    Operation::Connect,
                    BackendMsg::SessionAlreadyConnected,
                );
                return;
            }
            match DaSession::connect(endpoint.clone()) {
                Ok(session) => {
                    let info = session.info.clone();
                    send(
                        tx,
                        Event {
                            request_id,
                            session: session_id,
                            kind: EventKind::Connected { endpoint, info },
                        },
                    );
                    sessions.insert(session_id, session);
                }
                Err(message) => send(
                    tx,
                    Event {
                        request_id,
                        session: session_id,
                        kind: EventKind::Error {
                            operation: Operation::Connect,
                            message,
                        },
                    },
                ),
            }
        }
        CommandKind::Disconnect => {
            if let Some(session) = sessions.remove(&session_id) {
                let reason = session.shutdown();
                send(
                    tx,
                    Event {
                        request_id,
                        session: session_id,
                        kind: EventKind::Disconnected { reason },
                    },
                );
            }
        }
        CommandKind::Browse {
            node_id,
            filter: _,
            view_id: _,
        } => match sessions.get_mut(&session_id) {
            Some(session) => match session.browse(&node_id) {
                Ok(children) => send(
                    tx,
                    Event {
                        request_id,
                        session: session_id,
                        kind: EventKind::Browse { node_id, children },
                    },
                ),
                Err(message) => send_error(tx, request_id, session_id, Operation::Browse, message),
            },
            None => send_error(
                tx,
                request_id,
                session_id,
                Operation::Browse,
                BackendMsg::NoConnection,
            ),
        },
        CommandKind::BrowseViews => send_error(
            tx,
            request_id,
            session_id,
            Operation::BrowseViews,
            BackendMsg::DaNoViews,
        ),
        CommandKind::ResolveBrowsePath { .. } => send_error(
            tx,
            request_id,
            session_id,
            Operation::ResolveBrowsePath,
            BackendMsg::DaNoBrowsePath,
        ),
        CommandKind::AddItems { items } => match sessions.get_mut(&session_id) {
            Some(session) => {
                let results = session.add_items(&items);
                send(
                    tx,
                    Event {
                        request_id,
                        session: session_id,
                        kind: EventKind::AddItems { results },
                    },
                );
            }
            None => {
                let results = items
                    .into_iter()
                    .map(|item| (item.key, Err(BackendMsg::NoConnection)))
                    .collect();
                send(
                    tx,
                    Event {
                        request_id,
                        session: session_id,
                        kind: EventKind::AddItems { results },
                    },
                );
            }
        },
        CommandKind::RemoveItems { keys } => match sessions.get_mut(&session_id) {
            Some(session) => {
                session.remove_items(&keys);
                send(
                    tx,
                    Event {
                        request_id,
                        session: session_id,
                        kind: EventKind::Removed { keys },
                    },
                );
            }
            None => send_error(
                tx,
                request_id,
                session_id,
                Operation::Remove,
                BackendMsg::NoConnection,
            ),
        },
        CommandKind::Read { keys } => match sessions.get_mut(&session_id) {
            Some(session) => {
                let values = session.read(&keys);
                send(
                    tx,
                    Event {
                        request_id,
                        session: session_id,
                        kind: EventKind::Values { values },
                    },
                );
            }
            None => {
                let values = keys
                    .into_iter()
                    .map(|key| (key, DataValue::bad(BackendMsg::NoConnection)))
                    .collect();
                send(
                    tx,
                    Event {
                        request_id,
                        session: session_id,
                        kind: EventKind::Values { values },
                    },
                );
            }
        },
        CommandKind::Write { request } => match sessions.get_mut(&session_id) {
            Some(session) => {
                let key = request.key;
                let result = session.write(request);
                send(
                    tx,
                    Event {
                        request_id,
                        session: session_id,
                        kind: EventKind::Wrote { key, result },
                    },
                );
            }
            None => send_error(
                tx,
                request_id,
                session_id,
                Operation::Write,
                BackendMsg::NoConnection,
            ),
        },
        CommandKind::Test => match sessions.get_mut(&session_id) {
            Some(session) => match session.test() {
                Ok(message) => send(
                    tx,
                    Event {
                        request_id,
                        session: session_id,
                        kind: EventKind::Log { message },
                    },
                ),
                Err(message) => send_error(tx, request_id, session_id, Operation::Test, message),
            },
            None => send_error(
                tx,
                request_id,
                session_id,
                Operation::Test,
                BackendMsg::NoConnection,
            ),
        },
        // UA-only push subscription configuration; DA keeps periodic polling.
        CommandKind::ConfigureUaSubscription { .. } => {}
        // UA-only streaming subnet scan; DA has no equivalent command.
        CommandKind::ScanUaNetwork { .. } | CommandKind::CancelUaScan { .. } => {}
    }
}

fn send(tx: &Sender<Event>, event: Event) {
    let _ = tx.send(event);
}

fn send_error(
    tx: &Sender<Event>,
    request_id: u64,
    session: SessionId,
    operation: Operation,
    message: BackendMsg,
) {
    send(
        tx,
        Event {
            request_id,
            session,
            kind: EventKind::Error { operation, message },
        },
    );
}

/// GetStatus transfers both the structure and its optional vendor string.
struct ServerStatus(CoAllocation<crate::opc::bindings::da::tagOPCSERVERSTATUS>);

impl ServerStatus {
    /// # Safety
    /// `ptr` must be a non-null, uniquely owned GetStatus result.
    unsafe fn from_raw(ptr: *mut crate::opc::bindings::da::tagOPCSERVERSTATUS) -> Self {
        // SAFETY: the caller transfers the task allocation to this guard.
        Self(unsafe { CoAllocation::from_raw(ptr) })
    }

    fn get(&self) -> &crate::opc::bindings::da::tagOPCSERVERSTATUS {
        // SAFETY: construction requires a live non-null GetStatus allocation.
        unsafe { &*self.0.as_ptr() }
    }
}

impl Drop for ServerStatus {
    fn drop(&mut self) {
        // SAFETY: GetStatus transfers the vendor string with the structure.
        unsafe { drop(CoString::from_raw(self.get().szVendorInfo)) };
    }
}

/// Owns a registered server group until explicit shutdown or session drop.
struct ServerGroup {
    server: IOPCServer,
    handle: u32,
    armed: bool,
}

impl ServerGroup {
    fn remove(&mut self) {
        if self.armed {
            self.armed = false;
            // SAFETY: AddGroup succeeded and this guard still owns its handle.
            unsafe {
                let _ = self.server.RemoveGroup(self.handle, true);
            }
        }
    }
}

impl Drop for ServerGroup {
    fn drop(&mut self) {
        self.remove();
    }
}

fn create_group(
    server: &IOPCServer,
) -> Result<(ServerGroup, u32, IOPCItemMgt, IOPCSyncIO), BackendMsg> {
    let mut group_handle: u32 = 0;
    let mut revised: u32 = 0;
    let mut group_unknown: Option<IUnknown> = None;
    let group_name = wide("opcda-egui");
    // SAFETY: the server is live and all input and output pointers are valid.
    unsafe {
        server
            .AddGroup(
                PCWSTR(group_name.as_ptr()),
                true,
                GROUP_UPDATE_RATE_MS,
                1,
                std::ptr::null(),
                std::ptr::null(),
                0,
                &raw mut group_handle,
                &raw mut revised,
                &IOPCItemMgt::IID,
                &raw mut group_unknown,
            )
            .map_err(|e| BackendMsg::DaGroupCreateFailed {
                detail: e.to_string(),
            })?;
    }
    let pending_group = ServerGroup {
        server: server.clone(),
        handle: group_handle,
        armed: true,
    };
    let group_unknown = group_unknown.ok_or(BackendMsg::DaGroupEmpty)?;
    let item_mgt: IOPCItemMgt = group_unknown.cast().map_err(|e| BackendMsg::DaNoItemMgt {
        detail: e.to_string(),
    })?;
    let sync: IOPCSyncIO = group_unknown.cast().map_err(|e| BackendMsg::DaNoSyncIo {
        detail: e.to_string(),
    })?;

    Ok((pending_group, revised, item_mgt, sync))
}

impl DaSession {
    fn connect(endpoint: super::model::EndpointSpec) -> Result<Self, BackendMsg> {
        let prog_id = endpoint.address.trim();
        if prog_id.is_empty() {
            return Err(BackendMsg::DaProgIdMissing);
        }
        let clsid = resolve_server_clsid(&endpoint.host, prog_id)?;
        let unknown = create_instance(&clsid, &endpoint.host)?;
        let server: IOPCServer = unknown.cast().map_err(|e| BackendMsg::DaNoOpcServer {
            detail: e.to_string(),
        })?;
        let browse: Option<IOPCBrowseServerAddressSpace> = unknown.cast().ok();

        let (group, revised, item_mgt, sync) = create_group(&server)?;
        let group_handle = group.handle;

        let mut status_info = None;
        // SAFETY: the live server transfers the GetStatus result to the caller.
        if let Ok(status) = unsafe { server.GetStatus() } {
            if !status.is_null() {
                // SAFETY: a successful non-null GetStatus result is owned here.
                let status = unsafe { ServerStatus::from_raw(status) };
                let s = status.get();
                let vendor = if s.szVendorInfo.is_null() {
                    String::new()
                } else {
                    // SAFETY: GetStatus supplies a NUL-terminated vendor string.
                    unsafe { s.szVendorInfo.to_string().unwrap_or_default() }
                };
                status_info = Some((
                    s.wMajorVersion,
                    s.wMinorVersion,
                    s.wBuildNumber,
                    s.dwGroupCount,
                    vendor,
                ));
            }
        }
        let info = format_connect_info(
            group_handle,
            revised,
            status_info
                .as_ref()
                .map(
                    |(major, minor, build, group_count, vendor)| ServerStatusInfo {
                        major: *major,
                        minor: *minor,
                        build: *build,
                        group_count: *group_count,
                        vendor,
                    },
                )
                .as_ref(),
        );

        Ok(Self {
            endpoint,
            server,
            browse,
            item_mgt,
            sync,
            group,
            items: HashMap::new(),
            info,
        })
    }

    fn add_items(&mut self, items: &[ItemSpec]) -> Vec<(ItemKey, Result<ItemInfo, BackendMsg>)> {
        if items.is_empty() {
            return Vec::new();
        }
        let mut wide_ids: Vec<Vec<u16>> = items.iter().map(|i| wide(&i.item_id)).collect();
        let defs: Vec<tagOPCITEMDEF> = items
            .iter()
            .enumerate()
            .map(|(idx, item)| tagOPCITEMDEF {
                szAccessPath: PWSTR::null(),
                szItemID: PWSTR(wide_ids[idx].as_mut_ptr()),
                bActive: windows::core::BOOL(1),
                hClient: item.key,
                dwBlobSize: 0,
                pBlob: std::ptr::null_mut(),
                vtRequestedDataType: 0,
                wReserved: 0,
            })
            .collect();

        let mut results_ptr: *mut tagOPCITEMRESULT = std::ptr::null_mut();
        let mut errors_ptr: *mut HRESULT = std::ptr::null_mut();
        // SAFETY: definitions and output pointers remain valid for the COM call.
        let call = unsafe {
            self.item_mgt.AddItems(
                defs.len() as u32,
                defs.as_ptr(),
                &raw mut results_ptr,
                &raw mut errors_ptr,
            )
        };
        // SAFETY: COM transfers each non-null errors allocation to the caller.
        let _errors = unsafe { CoAllocation::from_raw(errors_ptr) };
        if let Err(e) = call {
            // SAFETY: a non-null returned block is task-allocated; failed calls
            // do not guarantee that individual result elements are initialized.
            let _results = unsafe { CoAllocation::from_raw(results_ptr) };
            return items
                .iter()
                .map(|i| (i.key, Err(BackendMsg::raw(format!("AddItems: {e}")))))
                .collect();
        }

        // SAFETY: successful AddItems returns one initialized result per item.
        let results = unsafe { ComArray::from_raw(results_ptr, items.len(), clear_item_result) };
        let results_ptr = results.as_ptr();
        let mut out = Vec::with_capacity(items.len());
        for (idx, item) in items.iter().enumerate() {
            // SAFETY: AddItems returned one error per input when non-null.
            let err = unsafe { read_hresult(errors_ptr, idx) };
            if err.is_err() {
                out.push((item.key, Err(BackendMsg::Raw(self.error_text(err)))));
                continue;
            }
            if results_ptr.is_null() {
                out.push((item.key, Err(BackendMsg::DaEmptyResult)));
                continue;
            }
            // SAFETY: successful AddItems returned one result per input.
            let result = unsafe { &*results_ptr.add(idx) };
            let slot = ItemSlot {
                server_handle: result.hServer,
            };
            let writable = result.dwAccessRights & OPC_WRITEABLE != 0;
            let canonical_vt = result.vtCanonicalDataType;
            self.items.insert(item.key, slot);
            out.push((
                item.key,
                Ok(ItemInfo {
                    data_type: Some(vt_name(canonical_vt).to_string()),
                    type_id: Some(canonical_vt),
                    writable,
                }),
            ));
        }

        out
    }

    fn remove_items(&mut self, keys: &[ItemKey]) {
        let handles: Vec<u32> = keys
            .iter()
            .filter_map(|key| self.items.get(key).map(|slot| slot.server_handle))
            .collect();
        if !handles.is_empty() {
            let mut errors: *mut HRESULT = std::ptr::null_mut();
            // SAFETY: handles and output pointer remain valid for RemoveItems.
            unsafe {
                let _ = self.item_mgt.RemoveItems(
                    handles.len() as u32,
                    handles.as_ptr(),
                    &raw mut errors,
                );
            }
            // SAFETY: RemoveItems transfers its nullable errors allocation.
            let _errors = unsafe { CoAllocation::from_raw(errors) };
        }
        for key in keys {
            self.items.remove(key);
        }
    }

    fn read(&mut self, keys: &[ItemKey]) -> Vec<(ItemKey, DataValue)> {
        if keys.is_empty() {
            return Vec::new();
        }
        let mut handles = Vec::with_capacity(keys.len());
        let mut resolved = Vec::with_capacity(keys.len());
        for key in keys {
            match self.items.get(key) {
                Some(slot) => {
                    handles.push(slot.server_handle);
                    resolved.push(*key);
                }
                None => resolved.push(*key),
            }
        }
        if handles.is_empty() {
            return keys
                .iter()
                .map(|key| (*key, DataValue::bad(BackendMsg::SignalNotSubscribed)))
                .collect();
        }

        let mut states: *mut tagOPCITEMSTATE = std::ptr::null_mut();
        let mut errors: *mut HRESULT = std::ptr::null_mut();
        // SAFETY: handles and output pointers remain valid for Read.
        let call = unsafe {
            self.sync.Read(
                OPC_DS_CACHE,
                handles.len() as u32,
                handles.as_ptr(),
                &raw mut states,
                &raw mut errors,
            )
        };
        // SAFETY: Read transfers its nullable error allocation to the caller.
        let _errors = unsafe { CoAllocation::from_raw(errors) };
        if let Err(e) = call {
            // On failure only the outer pointer can be freed: element initialization
            // is guaranteed by the OPC interface only on success.
            // SAFETY: a non-null returned block is task-allocated.
            let _states = unsafe { CoAllocation::from_raw(states) };
            let message = BackendMsg::Raw(self.error_text(e.code()));
            return resolved
                .iter()
                .map(|key| (*key, DataValue::bad(message.clone())))
                .collect();
        }

        // SAFETY: successful Read returns one initialized state per handle.
        let states = unsafe { ComArray::from_raw(states, handles.len(), clear_item_state) };
        let states = states.as_ptr();
        let mut out = Vec::with_capacity(resolved.len());
        let mut handle_idx = 0usize;
        for key in &resolved {
            if !self.items.contains_key(key) {
                out.push((*key, DataValue::bad(BackendMsg::SignalNotSubscribed)));
                continue;
            }
            // SAFETY: handle_idx indexes the returned per-handle error array.
            let err = unsafe { read_hresult(errors, handle_idx) };
            if err.is_err() || states.is_null() {
                out.push((*key, DataValue::bad(BackendMsg::Raw(self.error_text(err)))));
                handle_idx += 1;
                continue;
            }
            // SAFETY: successful Read returned an initialized state for each handle.
            let state = unsafe { &mut *states.add(handle_idx) };
            // SAFETY: the server returned an initialized VARIANT owned by the array.
            let value = unsafe { variant_to_value(&state.vDataValue) };
            out.push((
                *key,
                DataValue {
                    value,
                    quality: Quality::from_da(state.wQuality),
                    timestamp: filetime_to_utc(state.ftTimeStamp),
                    reason: None,
                },
            ));
            handle_idx += 1;
        }

        out
    }

    fn write(&mut self, request: super::model::WriteRequest) -> Result<(), BackendMsg> {
        let slot = self
            .items
            .get(&request.key)
            .ok_or(BackendMsg::SignalNotSubscribed)?;
        let variant = OwnedVariant(variant_from_value(&request.value, request.type_hint)?);
        let handles = [slot.server_handle];
        let mut errors: *mut HRESULT = std::ptr::null_mut();
        // SAFETY: handle, VARIANT, and output pointer remain live for Write.
        let call = unsafe {
            self.sync.Write(
                1,
                handles.as_ptr(),
                &variant.0 as *const VARIANT,
                &raw mut errors,
            )
        };
        // SAFETY: Write transfers its nullable errors allocation.
        let _errors = unsafe { CoAllocation::from_raw(errors) };
        match call {
            Err(e) => Err(BackendMsg::Raw(self.error_text(e.code()))),
            Ok(()) => {
                // SAFETY: a successful single-item Write supplies one error if non-null.
                let err = unsafe { read_hresult(errors, 0) };
                if err.is_err() {
                    Err(BackendMsg::Raw(self.error_text(err)))
                } else {
                    Ok(())
                }
            }
        }
    }

    fn browse(&mut self, node_id: &str) -> Result<Vec<BrowseNode>, BackendMsg> {
        let browse = self.browse.as_ref().ok_or(BackendMsg::DaNoBrowse)?;
        let empty = wide("");
        // SAFETY: the browse interface and terminated root path are live.
        unsafe {
            browse
                .ChangeBrowsePosition(OPC_BROWSE_TO, PCWSTR(empty.as_ptr()))
                .map_err(|e| BackendMsg::DaBrowseRootFailed {
                    detail: e.to_string(),
                })?;
        }

        let path: Vec<&str> = if node_id.is_empty() {
            Vec::new()
        } else {
            node_id.split('\u{1f}').filter(|s| !s.is_empty()).collect()
        };
        for segment in &path {
            let segment_w = wide(segment);
            // SAFETY: the browse interface and terminated path segment are live.
            unsafe {
                browse
                    .ChangeBrowsePosition(OPC_BROWSE_DOWN, PCWSTR(segment_w.as_ptr()))
                    .map_err(|e| BackendMsg::DaBrowseBranchFailed {
                        segment: (*segment).to_string(),
                        detail: e.to_string(),
                    })?;
            }
        }

        // SAFETY: the browse interface remains live for the query.
        let organization: tagOPCNAMESPACETYPE =
            unsafe { browse.QueryOrganization() }.map_err(|e| BackendMsg::DaNamespaceFailed {
                detail: e.to_string(),
            })?;

        let mut nodes = Vec::new();
        let mut seen = std::collections::HashSet::new();

        if organization != OPC_NS_FLAT {
            // SAFETY: the live browse interface returns owned enumeration strings.
            if let Ok(branches) = unsafe { browse_enum(browse, OPC_BRANCH) } {
                for name in branches {
                    if nodes.len() >= MAX_BROWSE_NODES {
                        break;
                    }
                    let mut path_parts = path.clone();
                    path_parts.push(&name);
                    let branch_id = path_parts.join("\u{1f}");
                    if seen.insert(branch_id.clone()) {
                        nodes.push(BrowseNode {
                            node_id: branch_id,
                            name,
                            kind: NodeKind::Branch,
                            has_children: true,
                        });
                    }
                }
            }
        }

        // SAFETY: the live browse interface returns owned enumeration strings.
        if let Ok(leaves) = unsafe { browse_enum(browse, OPC_LEAF) } {
            for name in leaves {
                if nodes.len() >= MAX_BROWSE_NODES {
                    break;
                }
                let name_w = wide(&name);
                // SAFETY: name_w is a terminated string live throughout the call.
                let item_id = unsafe { browse.GetItemID(PCWSTR(name_w.as_ptr())) }
                    .ok()
                    .map(|p| {
                        // SAFETY: GetItemID transfers its returned COM allocation.
                        unsafe { CoString::from_raw(p) }
                            .map_or_else(String::new, |owned| owned.text())
                    })
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| name.clone());
                if seen.insert(item_id.clone()) {
                    nodes.push(BrowseNode {
                        node_id: item_id,
                        name,
                        kind: NodeKind::Item,
                        has_children: false,
                    });
                }
            }
        }

        Ok(nodes)
    }

    fn test(&mut self) -> Result<BackendMsg, BackendMsg> {
        // SAFETY: the session owns a live server interface.
        let status = unsafe { self.server.GetStatus() }.map_err(|e| BackendMsg::DaLinkLost {
            detail: e.to_string(),
        })?;
        if status.is_null() {
            return Err(BackendMsg::DaNoState);
        }
        // SAFETY: successful GetStatus returned a non-null owned structure.
        let status = unsafe { ServerStatus::from_raw(status) };
        let state = status.get().dwServerState.0;
        check_server_state(state)
    }

    fn error_text(&self, hr: HRESULT) -> String {
        if hr.is_ok() {
            return "OK".to_string();
        }
        let mut message = format!("0x{:08X}", hr.0 as u32);
        // SAFETY: the session owns a live server interface.
        if let Ok(text) = unsafe { self.server.GetErrorString(hr, 0) } {
            // SAFETY: GetErrorString transfers its nullable task allocation.
            if let Some(text) = unsafe { CoString::from_raw(text) } {
                let localized = text.text();
                if !localized.is_empty() {
                    message.push_str(": ");
                    message.push_str(&localized);
                }
            }
        } else {
            message.push_str(": ");
            message.push_str(&self.endpoint.address);
        }
        message
    }

    fn shutdown(mut self) -> BackendMsg {
        self.group.remove();
        BackendMsg::ConnectionClosed
    }
}

unsafe fn browse_enum(
    browse: &IOPCBrowseServerAddressSpace,
    filter: crate::opc::bindings::da::tagOPCBROWSETYPE,
) -> Result<Vec<String>, BackendMsg> {
    let empty = wide("");
    // SAFETY: the interface is live and empty is a terminated filter string.
    let enumerator = unsafe { browse.BrowseOPCItemIDs(filter, PCWSTR(empty.as_ptr()), 0, 0) }
        .map_err(|e| BackendMsg::raw(format!("BrowseOPCItemIDs: {e}")))?;
    let mut out = Vec::new();
    loop {
        let mut buffer: [PWSTR; 32] = [PWSTR::null(); 32];
        let mut fetched: u32 = 0;
        // SAFETY: buffer and fetched are valid writable outputs for Next.
        let hr = unsafe { enumerator.Next(&mut buffer, Some(&raw mut fetched)) };
        if fetched == 0 {
            break;
        }
        for item in buffer.iter().take(fetched as usize) {
            // SAFETY: Next transfers each returned string to the caller.
            if let Some(item) = unsafe { CoString::from_raw(*item) } {
                if let Some(text) = item.try_text() {
                    out.push(text);
                }
            }
        }
        if out.len() >= MAX_BROWSE_NODES {
            break;
        }
        if hr.is_err() {
            break;
        }
    }
    Ok(out)
}

fn enumerate_servers(host: &str) -> Result<Vec<ServerEntry>, BackendMsg> {
    let unknown = create_instance(&CLSID_SERVER_LIST, host)
        .map_err(|e| BackendMsg::DaServerListFailed { cause: Box::new(e) })?;
    let list: IOPCServerList = unknown
        .cast()
        .map_err(|e| BackendMsg::DaServerListUnavailable {
            detail: e.to_string(),
        })?;

    let mut found: Vec<ServerEntry> = Vec::new();
    for catid in [
        crate::opc::bindings::da::CATID_OPCDAServer20::IID,
        crate::opc::bindings::da::CATID_OPCDAServer10::IID,
    ] {
        // SAFETY: the server list is live and category slices remain valid.
        let enumerator = match unsafe { list.EnumClassesOfCategories(&[catid], &[]) } {
            Ok(e) => e,
            Err(_) => continue,
        };
        loop {
            let mut guids: [GUID; 16] = [GUID::zeroed(); 16];
            let mut fetched: u32 = 0;
            // SAFETY: guids and fetched are valid writable outputs for Next.
            let hr = unsafe { enumerator.Next(&mut guids, Some(&raw mut fetched)) };
            if fetched == 0 {
                break;
            }
            for guid in guids.iter().take(fetched as usize) {
                if let Some(entry) = class_details(&list, guid) {
                    remember_discovered(host, &entry.id, guid);
                    if !found.iter().any(|e| e.id == entry.id) {
                        found.push(entry);
                    }
                }
            }
            if hr.is_err() {
                break;
            }
        }
    }

    found.sort_by(|a, b| a.id.to_lowercase().cmp(&b.id.to_lowercase()));
    if found.is_empty() {
        Err(BackendMsg::DaServersNotFound)
    } else {
        Ok(found)
    }
}

fn class_details(list: &IOPCServerList, clsid: &GUID) -> Option<ServerEntry> {
    let mut prog_id = PWSTR::null();
    let mut user_type = PWSTR::null();
    // SAFETY: the list and CLSID are live and output pointers are writable.
    let ok = unsafe { list.GetClassDetails(clsid, &raw mut prog_id, &raw mut user_type) }.is_ok();
    // SAFETY: GetClassDetails transfers each non-null output allocation.
    let prog = unsafe { CoString::from_raw(prog_id) }
        .map(|s| s.text())
        .unwrap_or_default();
    // SAFETY: the other output is a separate nullable COM allocation.
    let descr = unsafe { CoString::from_raw(user_type) }
        .map(|s| s.text())
        .unwrap_or_default();
    if !ok && prog.is_empty() {
        return None;
    }
    let id = if prog.is_empty() {
        // SAFETY: clsid references a valid GUID for the duration of the call.
        unsafe { windows::Win32::System::Com::ProgIDFromCLSID(clsid) }
            .ok()
            .map(|p| {
                // SAFETY: ProgIDFromCLSID transfers its returned COM allocation.
                unsafe { CoString::from_raw(p) }.map_or_else(String::new, |owned| owned.text())
            })
            .unwrap_or_default()
    } else {
        prog
    };
    if id.is_empty() {
        return None;
    }
    Some(ServerEntry {
        id,
        description: descr,
    })
}

fn create_instance(clsid: &GUID, host: &str) -> Result<IUnknown, BackendMsg> {
    let host = host.trim();
    if is_local_host(host) {
        // SAFETY: clsid is valid; COM reports initialization or activation failures.
        return unsafe {
            CoCreateInstance(clsid, None::<&IUnknown>, CLSCTX_SERVER)
                .map_err(|e| BackendMsg::raw(format!("CoCreateInstance: {e}")))
        };
    }

    let mut host_w = wide(host);
    let server_info = COSERVERINFO {
        dwReserved1: 0,
        pwszName: PWSTR(host_w.as_mut_ptr()),
        pAuthInfo: std::ptr::null_mut(),
        dwReserved2: 0,
    };
    let mut results = [MULTI_QI {
        pIID: &IUnknown::IID,
        pItf: ManuallyDrop::new(None),
        hr: HRESULT(0),
    }];
    // SAFETY: server_info, host string, IID and result storage live for the call;
    // successful pItf is taken exactly once from its ManuallyDrop wrapper.
    unsafe {
        CoCreateInstanceEx(
            clsid,
            None::<&IUnknown>,
            CLSCTX_LOCAL_SERVER | CLSCTX_REMOTE_SERVER,
            Some(&server_info),
            &mut results,
        )
        .map_err(|e| BackendMsg::raw(format!("CoCreateInstanceEx({host}): {e}")))?;
        if results[0].hr.is_err() {
            return Err(BackendMsg::raw(format!(
                "CoCreateInstanceEx({host}): 0x{:08X}",
                results[0].hr.0 as u32
            )));
        }
        let result = ManuallyDrop::take(&mut results[0].pItf);
        result.ok_or_else(|| BackendMsg::DaEmptyInterface {
            host: host.to_string(),
        })
    }
}

/// CLSIDs seen while enumerating servers (the model carries no CLSID, so the
/// connect step recovers it from here when the remote OPCEnum is unreachable).
fn discovered_clsids() -> &'static Mutex<DiscoveredClsids> {
    static CACHE: OnceLock<Mutex<DiscoveredClsids>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(DiscoveredClsids::default()))
}

fn remember_discovered(host: &str, prog_id: &str, clsid: &GUID) {
    discovered_clsids()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remember(host, prog_id, clsid.to_u128());
}

/// `CLSIDFromProgID` against the local registry.
fn clsid_from_local_registry(prog_id: &str) -> Result<GUID, BackendMsg> {
    let wide_id = wide(prog_id);
    // SAFETY: `wide_id` is a NUL-terminated UTF-16 buffer that outlives the call.
    unsafe { CLSIDFromProgID(PCWSTR(wide_id.as_ptr())) }.map_err(|e| {
        BackendMsg::DaProgIdLookupFailed {
            prog_id: prog_id.to_string(),
            detail: e.to_string(),
        }
    })
}

/// Ask the OPCEnum of `host` for the CLSID of `prog_id` (`IOPCServerList`,
/// then `IOPCServerList2` for enumerators that only expose the 2.0 interface).
fn clsid_from_remote_opcenum(host: &str, prog_id: &str) -> Result<GUID, BackendMsg> {
    let unknown = create_instance(&CLSID_SERVER_LIST, host).map_err(|e| {
        BackendMsg::DaOpcEnumUnavailable {
            host: host.to_string(),
            cause: Box::new(e),
        }
    })?;
    let wide_id = wide(prog_id);
    if let Ok(list) = unknown.cast::<IOPCServerList>() {
        // SAFETY: `wide_id` is a NUL-terminated UTF-16 buffer that outlives the call.
        return unsafe { list.CLSIDFromProgID(PCWSTR(wide_id.as_ptr())) }
            .map_err(|e| BackendMsg::raw(format!("IOPCServerList::CLSIDFromProgID: {e}")));
    }
    let list2: IOPCServerList2 =
        unknown
            .cast()
            .map_err(|e| BackendMsg::DaServerListHostUnavailable {
                host: host.to_string(),
                detail: e.to_string(),
            })?;
    // SAFETY: as above.
    unsafe { list2.CLSIDFromProgID(PCWSTR(wide_id.as_ptr())) }
        .map_err(|e| BackendMsg::raw(format!("IOPCServerList2::CLSIDFromProgID: {e}")))
}

struct WinClsidLookup;

impl ClsidLookup for WinClsidLookup {
    fn remote_enum(&mut self, host: &str, prog_id: &str) -> Result<u128, BackendMsg> {
        clsid_from_remote_opcenum(host, prog_id).map(|guid| guid.to_u128())
    }

    fn discovered(&self, host: &str, prog_id: &str) -> Option<u128> {
        discovered_clsids()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(host, prog_id)
    }

    fn local_registry(&mut self, prog_id: &str) -> Result<u128, BackendMsg> {
        clsid_from_local_registry(prog_id).map(|guid| guid.to_u128())
    }
}

/// CLSID of `prog_id` on `host`: the local registry for this computer, the
/// remote OPCEnum (then discovery cache, then local registry) for another one.
fn resolve_server_clsid(host: &str, prog_id: &str) -> Result<GUID, BackendMsg> {
    resolve_clsid(host, prog_id, &mut WinClsidLookup).map(GUID::from_u128)
}

unsafe fn clear_item_result(result: &mut tagOPCITEMRESULT) {
    // SAFETY: each result transfers its nullable pBlob task allocation.
    unsafe { drop(CoAllocation::from_raw(result.pBlob)) };
}

unsafe fn clear_item_state(state: &mut tagOPCITEMSTATE) {
    clear_variant(&mut state.vDataValue);
}

unsafe fn read_hresult(errors: *mut HRESULT, index: usize) -> HRESULT {
    if errors.is_null() {
        HRESULT(0)
    } else {
        // SAFETY: the caller guarantees index is in the returned error array.
        unsafe { *errors.add(index) }
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod ownership_tests {
    use super::*;

    #[windows::core::implement(IOPCServer)]
    struct MockServer {
        removes: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    impl crate::opc::bindings::da::IOPCServer_Impl for MockServer_Impl {
        fn AddGroup(
            &self,
            _name: &PCWSTR,
            _active: windows::core::BOOL,
            _rate: u32,
            _client: u32,
            _bias: *const i32,
            _deadband: *const f32,
            _locale: u32,
            handle: *mut u32,
            revised: *mut u32,
            _iid: *const GUID,
            output: windows::core::OutRef<'_, IUnknown>,
        ) -> windows::core::Result<()> {
            // SAFETY: AddGroup passes valid output pointers as required by the ABI.
            unsafe {
                *handle = 42;
                *revised = 1000;
            }
            // Return this server's IUnknown. It deliberately lacks IOPCItemMgt.
            output.write(Some(windows::core::IUnknownImpl::to_interface::<IUnknown>(
                self,
            )))
        }
        fn GetErrorString(&self, _: HRESULT, _: u32) -> windows::core::Result<PWSTR> {
            Err(windows::core::Error::from_hresult(HRESULT(
                0x80004001u32 as i32,
            )))
        }
        fn GetGroupByName(&self, _: &PCWSTR, _: *const GUID) -> windows::core::Result<IUnknown> {
            Err(windows::core::Error::from_hresult(HRESULT(
                0x80004001u32 as i32,
            )))
        }
        fn GetStatus(
            &self,
        ) -> windows::core::Result<*mut crate::opc::bindings::da::tagOPCSERVERSTATUS> {
            Err(windows::core::Error::from_hresult(HRESULT(
                0x80004001u32 as i32,
            )))
        }
        fn RemoveGroup(
            &self,
            handle: u32,
            force: windows::core::BOOL,
        ) -> windows::core::Result<()> {
            if handle != 42 || !force.as_bool() {
                return Err(windows::core::Error::from_hresult(HRESULT(
                    0x80070057u32 as i32,
                )));
            }
            self.removes
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
        fn CreateGroupEnumerator(
            &self,
            _: crate::opc::bindings::da::tagOPCENUMSCOPE,
            _: *const GUID,
        ) -> windows::core::Result<IUnknown> {
            Err(windows::core::Error::from_hresult(HRESULT(
                0x80004001u32 as i32,
            )))
        }
    }

    #[test]
    fn failed_group_interface_cast_removes_successfully_added_group_once() {
        let removes = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let server: IOPCServer = MockServer {
            removes: removes.clone(),
        }
        .into();
        assert!(matches!(
            create_group(&server),
            Err(BackendMsg::DaNoItemMgt { .. })
        ));
        assert_eq!(removes.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn server_group_shutdown_and_drop_remove_once() {
        let removes = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let server: IOPCServer = MockServer {
            removes: removes.clone(),
        }
        .into();
        {
            let mut group = ServerGroup {
                server,
                handle: 42,
                armed: true,
            };
            group.remove();
        }
        assert_eq!(removes.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn null_com_string_is_not_dereferenced() {
        // SAFETY: null is explicitly accepted and has no allocation to own.
        assert!(unsafe { CoString::from_raw(PWSTR::null()) }.is_none());
    }
}
