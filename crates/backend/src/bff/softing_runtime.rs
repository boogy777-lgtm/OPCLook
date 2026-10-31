//! Experimental safe owner for the direct Softing SDK C ABI. Not a production
//! backend yet: only the I4 fixture path is implemented.
#![deny(clippy::undocumented_unsafe_blocks)]
use std::collections::HashMap;
use std::ffi::{CString, c_char, c_void};
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

// Never reuse a registration across sessions or reconnect epochs.
static NEXT_REGISTRATION: AtomicU64 = AtomicU64::new(1);

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Guid {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}
#[repr(C)]
#[derive(Clone)]
struct BrowseRecord {
    is_branch: u32,
    name_len: u32,
    node_id_len: u32,
    name: [u8; 1024],
    node_id: [u8; 4096],
}
impl Default for BrowseRecord {
    fn default() -> Self {
        Self {
            is_branch: 0,
            name_len: 0,
            node_id_len: 0,
            name: [0; 1024],
            node_id: [0; 4096],
        }
    }
}
#[derive(Debug)]
pub struct BrowseEntry {
    pub name: String,
    pub node_id: String,
    pub is_branch: bool,
}
#[repr(C)]
#[derive(Clone)]
struct ServerRecord {
    clsid: Guid,
    progid_len: u32,
    description_len: u32,
    flags: u32,
    progid: [u8; 1024],
    description: [u8; 2048],
}
impl Default for ServerRecord {
    fn default() -> Self {
        Self {
            clsid: Guid::default(),
            progid_len: 0,
            description_len: 0,
            flags: 0,
            progid: [0; 1024],
            description: [0; 2048],
        }
    }
}
#[derive(Debug)]
pub struct DiscoveredServer {
    pub clsid: Guid,
    pub progid: String,
    pub description: String,
    pub description_missing: bool,
}
#[repr(C)]
#[derive(Default, Debug)]
pub struct ReadI4 {
    pub service_result: i32,
    pub item_result: i32,
    pub value: i32,
    pub filetime_low: u32,
    pub filetime_high: u32,
    pub quality: u16,
    pub vartype: u16,
}
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct ReadScalar {
    pub service_result: i32,
    pub item_result: i32,
    pub filetime_low: u32,
    pub filetime_high: u32,
    pub bits_low: u32,
    pub bits_high: u32,
    pub quality: u16,
    pub vartype: u16,
    pub text_len: u32,
}
pub const READ_SERVICE: u32 = 1;
pub const READ_ITEM: u32 = 2;
pub const READ_SAMPLE: u32 = 4;

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
struct NativeReadOutcome {
    value: ReadScalar,
    available: u32,
}

#[derive(Debug)]
pub enum ReadPayloadError {
    InvalidLength,
}

#[derive(Debug)]
pub struct ReadOutcome {
    pub adapter_result: i32,
    pub available: u32,
    pub value: ReadScalar,
    pub payload: Result<Option<Vec<u8>>, ReadPayloadError>,
}

#[repr(C)]
#[derive(Clone)]
struct PushRecord {
    registration_low: u32,
    registration_high: u32,
    master_quality: i32,
    adapter_result: i32,
    value: ReadScalar,
}
impl Default for PushRecord {
    fn default() -> Self {
        Self {
            registration_low: 0,
            registration_high: 0,
            master_quality: 0,
            adapter_result: 0,
            value: ReadScalar::default(),
        }
    }
}
pub struct PushValue {
    pub registration: u64,
    pub master_quality: i32,
    pub adapter_result: i32,
    pub value: ReadScalar,
    pub text: Option<Vec<u8>>,
}
unsafe extern "C" {
    fn opcl_da_browse(
        session: *mut c_void,
        node_id: *const c_char,
        out: *mut BrowseRecord,
        capacity: u32,
        count: *mut u32,
    ) -> i32;
    fn opcl_da_resolve(
        host: *mut c_void,
        node: *const c_char,
        progid: *const c_char,
        kind: u32,
        out: *mut Guid,
    ) -> i32;
    fn opcl_da_discover(
        host: *mut c_void,
        node: *const c_char,
        out: *mut ServerRecord,
        capacity: u32,
        count: *mut u32,
        fallback: *mut u32,
    ) -> i32;
    fn opcl_da_host_create(out: *mut *mut c_void) -> i32;
    fn opcl_da_host_destroy(host: *mut c_void) -> i32;
    fn opcl_da_session_open(
        host: *mut c_void,
        name: *const c_char,
        node: *const c_char,
        clsid: *const Guid,
        subscription: u32,
        update_rate_ms: u32,
        registration_budget: u32,
        revised_rate_ms: *mut u32,
        out: *mut *mut c_void,
    ) -> i32;
    fn opcl_da_session_close(session: *mut c_void) -> i32;
    fn opcl_da_server_status(session: *mut c_void, state: *mut i32) -> i32;
    fn opcl_da_item_add(
        session: *mut c_void,
        id: *const c_char,
        registration_low: u32,
        registration_high: u32,
        out: *mut *mut c_void,
        canonical_type: *mut u16,
        rights: *mut u32,
    ) -> i32;
    fn opcl_da_item_remove(session: *mut c_void, item: *mut c_void) -> i32;
    fn opcl_da_take_push(
        session: *mut c_void,
        out: *mut PushRecord,
        payload: *mut u8,
        capacity: u32,
        count: *mut u32,
        overflow: *mut u32,
    ) -> i32;
    fn opcl_da_read_i4(session: *mut c_void, item: *mut c_void, age: u32, out: *mut ReadI4) -> i32;
    fn opcl_da_read_scalar_facts(
        session: *mut c_void,
        item: *mut c_void,
        max_age: u32,
        out: *mut NativeReadOutcome,
        text: *mut c_char,
        text_capacity: u32,
    ) -> i32;
    fn opcl_da_write_array(
        session: *mut c_void,
        item: *mut c_void,
        vartype: u16,
        data: *const u8,
        bytes: u32,
        result: *mut i32,
    ) -> i32;
    fn opcl_da_write_text(
        session: *mut c_void,
        item: *mut c_void,
        text: *const c_char,
        result: *mut i32,
    ) -> i32;
    fn opcl_da_write_scalar(
        session: *mut c_void,
        item: *mut c_void,
        vt: u16,
        low: u32,
        high: u32,
        result: *mut i32,
    ) -> i32;
    #[cfg(feature = "native-da-test-hooks")]
    fn opcl_da_test_mixed_read(
        session: *mut c_void,
        first: *mut c_void,
        second: *mut c_void,
        out: *mut NativeReadOutcome,
    ) -> i32;
    #[cfg(feature = "native-da-test-hooks")]
    fn opcl_da_test_late_arm(value: i32) -> i32;
    #[cfg(feature = "native-da-test-hooks")]
    fn opcl_da_test_late_entered(timeout: u32) -> i32;
    #[cfg(feature = "native-da-test-hooks")]
    fn opcl_da_test_late_held() -> i32;
    #[cfg(feature = "native-da-test-hooks")]
    fn opcl_da_test_late_release() -> i32;
    #[cfg(feature = "native-da-test-hooks")]
    fn opcl_da_test_late_processed(timeout: u32) -> i32;
    #[cfg(feature = "native-da-test-hooks")]
    fn opcl_da_test_late_close() -> i32;
    #[cfg(feature = "native-da-test-hooks")]
    fn opcl_da_test_async_gate_arm() -> i32;
    #[cfg(feature = "native-da-test-hooks")]
    fn opcl_da_test_async_gate_entered(timeout: u32) -> i32;
    #[cfg(feature = "native-da-test-hooks")]
    fn opcl_da_test_async_gate_release() -> i32;
    #[cfg(feature = "native-da-test-hooks")]
    fn opcl_da_test_async_gate_held() -> i32;
    #[cfg(feature = "native-da-test-hooks")]
    fn opcl_da_test_async_gate_close() -> i32;
    #[cfg(feature = "native-da-test-hooks")]
    fn opcl_da_test_async_prepare(session: *mut c_void) -> i32;
    #[cfg(feature = "native-da-test-hooks")]
    fn opcl_da_test_async_read_value(
        session: *mut c_void,
        item: *mut c_void,
        age: u32,
        timeout: u32,
        out: *mut NativeReadOutcome,
        payload: *mut c_char,
        capacity: u32,
    ) -> i32;
    #[cfg(feature = "native-da-test-hooks")]
    fn opcl_da_test_async_read_i4(
        session: *mut c_void,
        item: *mut c_void,
        age: u32,
        timeout: u32,
        out: *mut ReadI4,
    ) -> i32;
    #[cfg(feature = "native-da-test-hooks")]
    fn opcl_da_test_async_wait_retire(
        session: *mut c_void,
        timeout: u32,
        out: *mut ReadI4,
        retired: *mut u32,
    ) -> i32;
    #[cfg(feature = "native-da-test-hooks")]
    fn opcl_da_test_async_last_token(session: *mut c_void, token: *mut u32) -> i32;
    #[cfg(feature = "native-da-test-hooks")]
    fn opcl_da_test_async_facts(
        session: *mut c_void,
        flags: *mut u32,
        submit: *mut i32,
        item: *mut i32,
    ) -> i32;
    #[cfg(feature = "native-da-test-hooks")]
    fn opcl_da_test_async_is_pending(session: *mut c_void, pending: *mut u32) -> i32;
    #[cfg(feature = "native-da-test-hooks")]
    fn opcl_da_test_arm_write_exception() -> i32;
    #[cfg(feature = "native-da-test-hooks")]
    fn opcl_da_test_write_exception_state() -> i32;
    fn opcl_da_write_i4(
        session: *mut c_void,
        item: *mut c_void,
        value: i32,
        result: *mut i32,
    ) -> i32;
}
#[cfg(feature = "native-da-test-hooks")]
pub fn test_arm_write_exception() -> Result<(), Error> {
    // SAFETY: test-only process-wide atomic hook takes no pointers.
    check(unsafe { opcl_da_test_arm_write_exception() })
}

#[cfg(feature = "native-da-test-hooks")]
pub fn test_write_exception_state() -> i32 {
    // SAFETY: test-only process-wide atomic hook takes no pointers.
    unsafe { opcl_da_test_write_exception_state() }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Native(i32),
    InvalidString,
    InvalidHandle,
    Exhausted,
    NullHandle,
    Busy,
}
fn check(hr: i32) -> Result<(), Error> {
    if hr < 0 {
        Err(Error::Native(hr))
    } else {
        Ok(())
    }
}
fn string(s: &str) -> Result<CString, Error> {
    if s.len() > 65_536 {
        return Err(Error::InvalidString);
    }
    CString::new(s).map_err(|_| Error::InvalidString)
}

// The SDK entry is initialized and terminated on the owner thread. A shared
// handle only keeps it alive for independently serialized session threads.
struct SharedCore {
    ptr: NonNull<c_void>,
}
// SAFETY: SDK entry branch lists are synchronized and each session is serialized;
// the owner only destroys the host after every SharedCore reference has gone.
unsafe impl Send for SharedCore {}
// SAFETY: concurrent callers create independent SDK server objects; the SDK
// synchronizes its entry list and server objects have separate worker threads.
unsafe impl Sync for SharedCore {}
#[derive(Clone)]
pub struct SharedHost {
    core: Arc<SharedCore>,
}
pub struct Host {
    core: Arc<SharedCore>,
    closed: bool,
    _thread: PhantomData<Rc<()>>,
}
impl Host {
    pub fn new() -> Result<Self, Error> {
        let mut ptr = std::ptr::null_mut();
        // SAFETY: the output pointer is valid and the SDK creates the host on this thread.
        check(unsafe { opcl_da_host_create(&mut ptr) })?;
        Ok(Self {
            core: Arc::new(SharedCore {
                ptr: NonNull::new(ptr).ok_or(Error::NullHandle)?,
            }),
            closed: false,
            _thread: PhantomData,
        })
    }
    pub fn shared(&self) -> SharedHost {
        SharedHost {
            core: Arc::clone(&self.core),
        }
    }
    pub fn is_idle(&self) -> bool {
        Arc::strong_count(&self.core) == 1
    }
    pub fn close(mut self) -> Result<(), Error> {
        self.closed = true;
        if Arc::strong_count(&self.core) != 1 {
            return Err(Error::Busy);
        }
        // SAFETY: owner thread and no shared/session references remain.
        check(unsafe { opcl_da_host_destroy(self.core.ptr.as_ptr()) })
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        if !self.closed && Arc::strong_count(&self.core) == 1 {
            // SAFETY: owner thread and no shared/session references remain.
            let _ = unsafe { opcl_da_host_destroy(self.core.ptr.as_ptr()) };
        }
        // A detached session can still use the host. Leave the native host
        // alive rather than terminate it on another thread or free it early.
    }
}
impl SharedHost {
    /// Test instrumentation: identity of the Arc actually handed to a Gateway.
    #[cfg(feature = "native-da-test-hooks")]
    pub fn test_identity(&self) -> usize {
        Arc::as_ptr(&self.core) as usize
    }
    #[cfg(test)]
    pub fn same_host(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.core, &other.core)
    }
    pub fn resolve(&self, node: &str, progid: &str, remote: bool) -> Result<Guid, Error> {
        let node = string(node)?;
        let progid = string(progid)?;
        let mut out = Guid::default();
        // SAFETY: the shared host remains live and the inputs and output live through this synchronous call.
        check(unsafe {
            opcl_da_resolve(
                self.core.ptr.as_ptr(),
                node.as_ptr(),
                progid.as_ptr(),
                u32::from(remote),
                &mut out,
            )
        })?;
        Ok(out)
    }
    pub fn discover(&self, node: &str) -> Result<(Vec<DiscoveredServer>, bool), Error> {
        let node = string(node)?;
        let mut records = vec![ServerRecord::default(); 256];
        let mut count = 0;
        let mut fallback = 0;
        // SAFETY: the host is live, records owns capacity entries, and every output remains valid for the call.
        check(unsafe {
            opcl_da_discover(
                self.core.ptr.as_ptr(),
                node.as_ptr(),
                records.as_mut_ptr(),
                256,
                &mut count,
                &mut fallback,
            )
        })?;
        let count = usize::try_from(count).map_err(|_| Error::InvalidString)?;
        if count > records.len() || fallback > 1 {
            return Err(Error::InvalidString);
        }
        let mut found = Vec::with_capacity(count);
        for record in records.into_iter().take(count) {
            let progid = record
                .progid
                .get(..record.progid_len as usize)
                .ok_or(Error::InvalidString)?;
            let description = record
                .description
                .get(..record.description_len as usize)
                .ok_or(Error::InvalidString)?;
            found.push(DiscoveredServer {
                clsid: record.clsid,
                progid: String::from_utf8(progid.to_vec()).map_err(|_| Error::InvalidString)?,
                description: String::from_utf8(description.to_vec())
                    .map_err(|_| Error::InvalidString)?,
                description_missing: record.flags & 1 != 0,
            });
        }
        Ok((found, fallback != 0))
    }
    pub fn open(&self, name: &str, node: &str, clsid: &Guid) -> Result<Session, Error> {
        self.open_config(name, node, clsid, false, 1000)
            .map(|(session, _)| session)
    }
    pub fn open_config(
        &self,
        name: &str,
        node: &str,
        clsid: &Guid,
        subscription: bool,
        interval_ms: u32,
    ) -> Result<(Session, u32), Error> {
        let name = string(name)?;
        let node = string(node)?;
        let mut ptr = std::ptr::null_mut();
        let mut revised = 0;
        let budget = u32::try_from(opclook_contract::model::MAX_ITEM_KEYS_PER_CONNECTION)
            .map_err(|_| Error::Exhausted)?;
        // SAFETY: self retains the live host, C strings and outputs live through the call.
        check(unsafe {
            opcl_da_session_open(
                self.core.ptr.as_ptr(),
                name.as_ptr(),
                node.as_ptr(),
                clsid,
                u32::from(subscription),
                interval_ms,
                budget,
                &mut revised,
                &mut ptr,
            )
        })?;
        Ok((
            Session {
                ptr: NonNull::new(ptr).ok_or(Error::NullHandle)?,
                _host: self.clone(),
                items: HashMap::new(),
                push_buffer: if subscription {
                    vec![0; 65_536]
                } else {
                    Vec::new()
                },
                closed: false,
                _thread: PhantomData,
            },
            revised,
        ))
    }
}
pub struct Session {
    ptr: NonNull<c_void>,
    _host: SharedHost,
    items: HashMap<u64, NonNull<c_void>>,
    push_buffer: Vec<u8>,
    closed: bool,
    _thread: PhantomData<Rc<()>>,
}
#[cfg(feature = "native-da-test-hooks")]
pub fn test_late_arm(value: i32) -> Result<(), Error> {
    // SAFETY: the test serializes event lifecycle around the held SDK callback.
    check(unsafe { opcl_da_test_late_arm(value) })
}
#[cfg(feature = "native-da-test-hooks")]
pub fn test_late_entered(timeout: u32) -> Result<(), Error> {
    // SAFETY: the test serializes event lifecycle around the held SDK callback.
    check(unsafe { opcl_da_test_late_entered(timeout) })
}
#[cfg(feature = "native-da-test-hooks")]
pub fn test_late_held() -> Result<(), Error> {
    // SAFETY: the test serializes event lifecycle around the held SDK callback.
    check(unsafe { opcl_da_test_late_held() })
}
#[cfg(feature = "native-da-test-hooks")]
pub fn test_late_release() -> Result<(), Error> {
    // SAFETY: the test serializes event lifecycle around the held SDK callback.
    check(unsafe { opcl_da_test_late_release() })
}
#[cfg(feature = "native-da-test-hooks")]
pub fn test_late_processed(timeout: u32) -> Result<(), Error> {
    // SAFETY: the test serializes event lifecycle around the held SDK callback.
    check(unsafe { opcl_da_test_late_processed(timeout) })
}
#[cfg(feature = "native-da-test-hooks")]
pub fn test_late_close() -> Result<(), Error> {
    // SAFETY: the test serializes event lifecycle around the held SDK callback.
    check(unsafe { opcl_da_test_late_close() })
}
#[cfg(feature = "native-da-test-hooks")]
pub fn test_async_gate_arm() -> Result<(), Error> {
    // SAFETY: test-only process event hook takes no pointers; the session owner serializes use.
    check(unsafe { opcl_da_test_async_gate_arm() })
}
#[cfg(feature = "native-da-test-hooks")]
pub fn test_async_gate_entered(timeout: u32) -> Result<(), Error> {
    // SAFETY: test-only event remains armed until explicitly closed.
    check(unsafe { opcl_da_test_async_gate_entered(timeout) })
}
#[cfg(feature = "native-da-test-hooks")]
pub fn test_async_gate_held() -> Result<(), Error> {
    // SAFETY: the test owner keeps the event armed until task retirement.
    check(unsafe { opcl_da_test_async_gate_held() })
}
#[cfg(feature = "native-da-test-hooks")]
pub fn test_async_gate_release() -> Result<(), Error> {
    // SAFETY: test-only event remains armed until explicitly closed.
    check(unsafe { opcl_da_test_async_gate_release() })
}
#[cfg(feature = "native-da-test-hooks")]
pub fn test_async_gate_close() -> Result<(), Error> {
    // SAFETY: caller closes the test event only after the SDK task is retired.
    check(unsafe { opcl_da_test_async_gate_close() })
}
/// Test-only quarantine of the actual session and its SharedHost Arc.
/// Dropping while SDK work is pending intentionally retains both indefinitely.
#[cfg(feature = "native-da-test-hooks")]
pub struct TestAsyncSessionLease {
    session: Option<Session>,
    pending: bool,
}
#[cfg(feature = "native-da-test-hooks")]
impl TestAsyncSessionLease {
    pub fn new(session: Session) -> Self {
        Self {
            session: Some(session),
            pending: false,
        }
    }
    pub fn session_mut(&mut self) -> Option<&mut Session> {
        if self.pending {
            None
        } else {
            self.session.as_mut()
        }
    }
    pub fn last_token(&self) -> Result<u32, Error> {
        let session = self.session.as_ref().ok_or(Error::InvalidHandle)?;
        let mut token = 0;
        // SAFETY: the lease retains the session; the SDK slot serializes token access.
        check(unsafe { opcl_da_test_async_last_token(session.ptr.as_ptr(), &mut token) })?;
        Ok(token)
    }
    pub fn facts(&self) -> Result<(u32, i32, i32), Error> {
        let session = self.session.as_ref().ok_or(Error::InvalidHandle)?;
        let (mut flags, mut submit, mut item) = (0, 0, 0);
        // SAFETY: the lease owns the live session; outputs are writable for this call.
        check(unsafe {
            opcl_da_test_async_facts(session.ptr.as_ptr(), &mut flags, &mut submit, &mut item)
        })?;
        Ok((flags, submit, item))
    }
    pub fn prepare(&mut self) -> Result<(), Error> {
        let session = self.session.as_ref().ok_or(Error::InvalidHandle)?;
        // SAFETY: the lease uniquely owns the live session and serializes the call.
        check(unsafe { opcl_da_test_async_prepare(session.ptr.as_ptr()) })
    }
    pub fn read(&mut self, key: u64, age: u32, timeout: u32) -> Result<(i32, ReadI4), Error> {
        if self.pending {
            return Err(Error::Busy);
        }
        let session = self.session.as_ref().ok_or(Error::InvalidHandle)?;
        let item = session.items.get(&key).ok_or(Error::InvalidHandle)?;
        let mut out = ReadI4::default();
        // SAFETY: session and item are uniquely owned and all outputs remain valid during the call.
        let hr = unsafe {
            opcl_da_test_async_read_i4(session.ptr.as_ptr(), item.as_ptr(), age, timeout, &mut out)
        };
        let mut pending = 1;
        // SAFETY: the session remains owned and pending is valid for the call.
        let status = unsafe { opcl_da_test_async_is_pending(session.ptr.as_ptr(), &mut pending) };
        self.pending = status < 0 || pending != 0;
        Ok((hr, out))
    }
    pub fn read_value(&mut self, key: u64, age: u32, timeout: u32) -> Result<ReadOutcome, Error> {
        if self.pending {
            return Err(Error::Busy);
        }
        let session = self.session.as_ref().ok_or(Error::InvalidHandle)?;
        let item = session.items.get(&key).ok_or(Error::InvalidHandle)?;
        let mut out = NativeReadOutcome::default();
        let mut payload = vec![0u8; 65_536];
        // SAFETY: the unique lease keeps session/item live; buffers are used only until this call returns.
        let adapter_result = unsafe {
            opcl_da_test_async_read_value(
                session.ptr.as_ptr(),
                item.as_ptr(),
                age,
                timeout,
                &mut out,
                payload.as_mut_ptr().cast(),
                payload.len() as u32,
            )
        };
        let mut pending = 1;
        // SAFETY: session remains owned by this lease and pending is writable.
        let status = unsafe { opcl_da_test_async_is_pending(session.ptr.as_ptr(), &mut pending) };
        self.pending = status < 0 || pending != 0;
        let payload = if adapter_result >= 0
            && out.available & READ_SAMPLE != 0
            && (out.value.vartype == 8 || out.value.vartype & 0x2000 != 0)
        {
            match usize::try_from(out.value.text_len) {
                Ok(count) if count <= payload.len() => {
                    payload.truncate(count);
                    Ok(Some(payload))
                }
                _ => Err(ReadPayloadError::InvalidLength),
            }
        } else {
            Ok(None)
        };
        Ok(ReadOutcome {
            adapter_result,
            available: out.available,
            value: out.value,
            payload,
        })
    }
    pub fn wait_retire(&mut self, timeout: u32) -> Result<(i32, ReadI4), Error> {
        let session = self.session.as_ref().ok_or(Error::InvalidHandle)?;
        let mut out = ReadI4::default();
        let mut retired = 0;
        // SAFETY: the lease keeps the session and host live, and both outputs are valid.
        let hr = unsafe {
            opcl_da_test_async_wait_retire(session.ptr.as_ptr(), timeout, &mut out, &mut retired)
        };
        if retired == 1 {
            self.pending = false;
        }
        Ok((hr, out))
    }
    pub fn into_session(mut self) -> Result<Session, Self> {
        if self.pending {
            return Err(self);
        }
        match self.session.take() {
            Some(session) => Ok(session),
            None => Err(self),
        }
    }
}
#[cfg(feature = "native-da-test-hooks")]
impl Drop for TestAsyncSessionLease {
    fn drop(&mut self) {
        if self.pending {
            if let Some(session) = self.session.take() {
                std::mem::forget(session);
            }
        }
    }
}
impl Session {
    #[cfg(feature = "native-da-test-hooks")]
    pub(super) fn test_mixed_read(
        &mut self,
        first: u64,
        second: u64,
    ) -> Result<[ReadOutcome; 2], Error> {
        let first = self.items.get(&first).ok_or(Error::InvalidHandle)?;
        let second = self.items.get(&second).ok_or(Error::InvalidHandle)?;
        let mut out = [NativeReadOutcome::default(), NativeReadOutcome::default()];
        // SAFETY: both registered SDK items and both output records live through the synchronous call.
        check(unsafe {
            opcl_da_test_mixed_read(
                self.ptr.as_ptr(),
                first.as_ptr(),
                second.as_ptr(),
                out.as_mut_ptr(),
            )
        })?;
        Ok(out.map(|raw| ReadOutcome {
            adapter_result: 0,
            available: raw.available,
            value: raw.value,
            payload: Ok(None),
        }))
    }
    pub fn test_status(&mut self) -> Result<i32, Error> {
        let mut state = 0;
        // SAFETY: this owner serializes access to its live session, and the
        // output remains valid until the SDK worker call returns.
        check(unsafe { opcl_da_server_status(self.ptr.as_ptr(), &mut state) })?;
        Ok(state)
    }

    #[cfg(test)]
    pub(crate) fn inject_mock_push(&self, registration: u64, value: i32, overflow: bool) {
        tests::inject(self.ptr.as_ptr() as usize, registration, value, overflow);
    }
    pub fn take_push(&mut self) -> Result<(Vec<PushValue>, bool), Error> {
        if self.push_buffer.is_empty() {
            return Err(Error::InvalidHandle);
        }
        let mut values = Vec::new();
        let mut any_overflow = false;
        for _ in 0..64 {
            let mut raw = PushRecord::default();
            let mut count = 0;
            let mut overflow = 0;
            // SAFETY: session is unique and the reusable payload and all outputs remain live.
            check(unsafe {
                opcl_da_take_push(
                    self.ptr.as_ptr(),
                    &mut raw,
                    self.push_buffer.as_mut_ptr(),
                    self.push_buffer.len() as u32,
                    &mut count,
                    &mut overflow,
                )
            })?;
            if count > 1 || overflow > 1 {
                return Err(Error::InvalidString);
            }
            any_overflow |= overflow != 0;
            if count == 0 {
                break;
            }
            let registration =
                (u64::from(raw.registration_high) << 32) | u64::from(raw.registration_low);
            let text = if raw.adapter_result >= 0
                && (raw.value.vartype == 8 || raw.value.vartype & 0x2000 != 0)
                && raw.value.service_result >= 0
                && raw.value.item_result >= 0
            {
                let count =
                    usize::try_from(raw.value.text_len).map_err(|_| Error::InvalidString)?;
                Some(
                    self.push_buffer
                        .get(..count)
                        .ok_or(Error::InvalidString)?
                        .to_vec(),
                )
            } else {
                None
            };
            values.push(PushValue {
                registration,
                master_quality: raw.master_quality,
                adapter_result: raw.adapter_result,
                value: raw.value,
                text,
            });
        }
        Ok((values, any_overflow))
    }
    pub fn browse(&mut self, node_id: &str) -> Result<Vec<BrowseEntry>, Error> {
        let node_id = string(node_id)?;
        let mut records = vec![BrowseRecord::default(); 256];
        let mut count = 0;
        // SAFETY: the session is uniquely owned and the input/output buffers live until this synchronous call returns.
        check(unsafe {
            opcl_da_browse(
                self.ptr.as_ptr(),
                node_id.as_ptr(),
                records.as_mut_ptr(),
                256,
                &mut count,
            )
        })?;
        let count = usize::try_from(count).map_err(|_| Error::InvalidString)?;
        if count > records.len() {
            return Err(Error::InvalidString);
        }
        let mut result = Vec::with_capacity(count);
        for record in records.into_iter().take(count) {
            if record.is_branch > 1 {
                return Err(Error::InvalidString);
            }
            let name = record
                .name
                .get(..record.name_len as usize)
                .ok_or(Error::InvalidString)?;
            let id = record
                .node_id
                .get(..record.node_id_len as usize)
                .ok_or(Error::InvalidString)?;
            result.push(BrowseEntry {
                name: String::from_utf8(name.to_vec()).map_err(|_| Error::InvalidString)?,
                node_id: String::from_utf8(id.to_vec()).map_err(|_| Error::InvalidString)?,
                is_branch: record.is_branch == 1,
            });
        }
        Ok(result)
    }
    pub fn add(&mut self, id: &str) -> Result<(u64, u16, u32), Error> {
        let key = NEXT_REGISTRATION
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| Error::Exhausted)?;
        let id = string(id)?;
        let mut item = std::ptr::null_mut();
        let mut kind = 0;
        let mut rights = 0;
        // SAFETY: the session is live and serialized, strings and outputs live for the call.
        check(unsafe {
            opcl_da_item_add(
                self.ptr.as_ptr(),
                id.as_ptr(),
                key as u32,
                (key >> 32) as u32,
                &mut item,
                &mut kind,
                &mut rights,
            )
        })?;
        let item = NonNull::new(item).ok_or(Error::NullHandle)?;
        self.items.insert(key, item);
        Ok((key, kind, rights))
    }
    pub fn remove(&mut self, key: u64) -> Result<(), Error> {
        let item = self.items.remove(&key).ok_or(Error::InvalidHandle)?;
        // SAFETY: item belonged to this live session and was removed from the map first.
        check(unsafe { opcl_da_item_remove(self.ptr.as_ptr(), item.as_ptr()) })
    }
    pub fn read_i4(&mut self, key: u64, max_age: u32) -> Result<ReadI4, Error> {
        let item = self.items.get(&key).ok_or(Error::InvalidHandle)?;
        let mut out = ReadI4::default();
        // SAFETY: the map owns the live registration; out is valid for the synchronous call.
        check(unsafe { opcl_da_read_i4(self.ptr.as_ptr(), item.as_ptr(), max_age, &mut out) })?;
        Ok(out)
    }
    pub fn read_scalar(&mut self, key: u64, max_age: u32) -> Result<ReadOutcome, Error> {
        let item = self.items.get(&key).ok_or(Error::InvalidHandle)?;
        let mut out = NativeReadOutcome::default();
        let mut text = vec![0u8; 65_536];
        // SAFETY: the live item and bounded output buffers outlive the synchronous call.
        let adapter_result = unsafe {
            opcl_da_read_scalar_facts(
                self.ptr.as_ptr(),
                item.as_ptr(),
                max_age,
                &mut out,
                text.as_mut_ptr().cast(),
                text.len() as u32,
            )
        };
        let payload = if adapter_result >= 0
            && out.available & READ_SAMPLE != 0
            && (out.value.vartype == 8 || out.value.vartype & 0x2000 != 0)
        {
            match usize::try_from(out.value.text_len) {
                Ok(count) if count <= text.len() => {
                    text.truncate(count);
                    Ok(Some(text))
                }
                _ => Err(ReadPayloadError::InvalidLength),
            }
        } else {
            Ok(None)
        };
        Ok(ReadOutcome {
            adapter_result,
            available: out.available,
            value: out.value,
            payload,
        })
    }
    pub fn write_array(&mut self, key: u64, vt: u16, bytes: &[u8]) -> Result<i32, Error> {
        let item = self.items.get(&key).ok_or(Error::InvalidHandle)?;
        let size = u32::try_from(bytes.len()).map_err(|_| Error::Exhausted)?;
        let mut result = 0;
        // SAFETY: item and borrowed byte slice remain live through the synchronous SDK task.
        check(unsafe {
            opcl_da_write_array(
                self.ptr.as_ptr(),
                item.as_ptr(),
                vt,
                bytes.as_ptr(),
                size,
                &mut result,
            )
        })?;
        Ok(result)
    }
    pub fn write_text(&mut self, key: u64, value: &str) -> Result<i32, Error> {
        let item = self.items.get(&key).ok_or(Error::InvalidHandle)?;
        let text = string(value)?;
        let mut result = 0;
        // SAFETY: the registration and the C string and output outlive the call.
        check(unsafe {
            opcl_da_write_text(self.ptr.as_ptr(), item.as_ptr(), text.as_ptr(), &mut result)
        })?;
        Ok(result)
    }
    pub fn write_scalar(&mut self, key: u64, vt: u16, bits: u64) -> Result<i32, Error> {
        let item = self.items.get(&key).ok_or(Error::InvalidHandle)?;
        let mut result = 0;
        // SAFETY: the map owns the registration and the output remains valid for the synchronous call.
        check(unsafe {
            opcl_da_write_scalar(
                self.ptr.as_ptr(),
                item.as_ptr(),
                vt,
                bits as u32,
                (bits >> 32) as u32,
                &mut result,
            )
        })?;
        Ok(result)
    }
    pub fn write_i4(&mut self, key: u64, value: i32) -> Result<i32, Error> {
        let item = self.items.get(&key).ok_or(Error::InvalidHandle)?;
        let mut result = 0;
        // SAFETY: the map owns the live registration and the output remains live for the call.
        check(unsafe { opcl_da_write_i4(self.ptr.as_ptr(), item.as_ptr(), value, &mut result) })?;
        Ok(result)
    }
    pub fn close(mut self) -> Result<(), Error> {
        self.closed = true;
        // SAFETY: this unique session is consumed and closed exactly once.
        check(unsafe { opcl_da_session_close(self.ptr.as_ptr()) })
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        if !self.closed {
            // SAFETY: each session is unique, serialized, and is closed exactly once here.
            let _ = unsafe { opcl_da_session_close(self.ptr.as_ptr()) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Mutex, OnceLock};
    type MockQueues = HashMap<usize, (VecDeque<PushRecord>, bool)>;
    static QUEUES: OnceLock<Mutex<MockQueues>> = OnceLock::new();
    static STATUS: OnceLock<Mutex<HashMap<usize, i32>>> = OnceLock::new();
    fn statuses() -> &'static Mutex<HashMap<usize, i32>> {
        STATUS.get_or_init(|| Mutex::new(HashMap::new()))
    }
    fn queues() -> &'static Mutex<MockQueues> {
        QUEUES.get_or_init(|| Mutex::new(HashMap::new()))
    }
    pub(super) fn inject(session: usize, registration: u64, value: i32, overflow: bool) {
        let mut queue = queues()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let entry = queue.entry(session).or_default();
        let mut record = PushRecord::default();
        record.registration_low = registration as u32;
        record.registration_high = (registration >> 32) as u32;
        record.value.vartype = 3;
        record.value.quality = 0xc0;
        record.value.bits_low = value as u32;
        record.value.bits_high = if value < 0 { u32::MAX } else { 0 };
        entry.0.push_back(record);
        entry.1 |= overflow;
    }
    static HANDLE: AtomicUsize = AtomicUsize::new(100);
    fn handle() -> *mut c_void {
        HANDLE.fetch_add(1, Ordering::Relaxed) as *mut c_void
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn opcl_da_browse(
        _: *mut c_void,
        path: *const c_char,
        out: *mut BrowseRecord,
        capacity: u32,
        count: *mut u32,
    ) -> i32 {
        // SAFETY: the wrapper supplies a valid NUL-terminated string and writable output pointers.
        unsafe {
            if capacity == 0 {
                return -1;
            }
            let name = if std::ffi::CStr::from_ptr(path).to_bytes().is_empty() {
                b"Fixture".as_slice()
            } else {
                b"Value".as_slice()
            };
            let id = if name == b"Fixture" {
                b"Fixture".as_slice()
            } else {
                b"Fixture.Value".as_slice()
            };
            let record = &mut *out;
            record.name[..name.len()].copy_from_slice(name);
            record.node_id[..id.len()].copy_from_slice(id);
            record.name_len = name.len() as u32;
            record.node_id_len = id.len() as u32;
            record.is_branch = u32::from(name == b"Fixture");
            *count = 1;
        }
        0
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn opcl_da_resolve(
        _: *mut c_void,
        _: *const c_char,
        _: *const c_char,
        _: u32,
        out: *mut Guid,
    ) -> i32 {
        // SAFETY: the wrapper supplies a valid output pointer.
        unsafe {
            *out = Guid {
                data1: 1,
                ..Guid::default()
            };
        }
        0
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn opcl_da_discover(
        _: *mut c_void,
        _: *const c_char,
        out: *mut ServerRecord,
        capacity: u32,
        count: *mut u32,
        fallback: *mut u32,
    ) -> i32 {
        // SAFETY: the wrapper supplies at least one record and valid output pointers.
        unsafe {
            if capacity == 0 {
                return -1;
            }
            let record = &mut *out;
            let id = b"OPCLook.NativeDaFixture.1";
            record.progid[..id.len()].copy_from_slice(id);
            record.progid_len = id.len() as u32;
            record.clsid = Guid {
                data1: 1,
                ..Guid::default()
            };
            *count = 1;
            *fallback = 1;
        }
        0
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn opcl_da_host_create(out: *mut *mut c_void) -> i32 {
        // SAFETY: the wrapper passes a valid out pointer.
        unsafe { *out = handle() };
        0
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn opcl_da_host_destroy(_: *mut c_void) -> i32 {
        0
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn opcl_da_session_open(
        _: *mut c_void,
        _: *const c_char,
        _: *const c_char,
        guid: *const Guid,
        _: u32,
        _: u32,
        _: u32,
        revised: *mut u32,
        out: *mut *mut c_void,
    ) -> i32 {
        // SAFETY: the wrapper passes valid output pointers.
        unsafe {
            let session = handle();
            *out = session;
            *revised = 1000;
            let test_state = match (*guid).data1 {
                0xfedc0002 => 2,
                0xfedc0003 => 3,
                0xfedc0004 => 4,
                0xfedc0005 => 5,
                0xfedc0006 => 6,
                0xfedc002a => 42,
                0xfedc0000 => 0,
                0xfedcdead => i32::MIN + 5,
                _ => 1,
            };
            statuses()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(session as usize, test_state);
        };
        0
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn opcl_da_session_close(session: *mut c_void) -> i32 {
        statuses()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&(session as usize));
        0
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn opcl_da_server_status(session: *mut c_void, state: *mut i32) -> i32 {
        let outcome = statuses()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&(session as usize))
            .copied()
            .unwrap_or(i32::MIN);
        if outcome < 0 {
            return outcome;
        }
        // SAFETY: the wrapper supplies a valid output pointer for this call.
        unsafe { *state = outcome };
        0
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn opcl_da_item_add(
        _: *mut c_void,
        _: *const c_char,
        _: u32,
        _: u32,
        out: *mut *mut c_void,
        kind: *mut u16,
        rights: *mut u32,
    ) -> i32 {
        // SAFETY: the wrapper passes valid output pointers.
        unsafe {
            *out = handle();
            *kind = 3;
            *rights = 2;
        }
        0
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn opcl_da_item_remove(_: *mut c_void, _: *mut c_void) -> i32 {
        0
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn opcl_da_take_push(
        session: *mut c_void,
        out: *mut PushRecord,
        _: *mut u8,
        _: u32,
        count: *mut u32,
        overflow: *mut u32,
    ) -> i32 {
        let mut queues = queues()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let entry = queues.entry(session as usize).or_default();
        let record = entry.0.pop_front();
        // SAFETY: the wrapper supplies valid output pointers.
        unsafe {
            *count = u32::from(record.is_some());
            *overflow = u32::from(entry.1);
            if let Some(record) = record {
                *out = record;
            }
        }
        entry.1 = false;
        0
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn opcl_da_read_i4(
        _: *mut c_void,
        _: *mut c_void,
        _: u32,
        out: *mut ReadI4,
    ) -> i32 {
        // SAFETY: the wrapper passes a valid output pointer.
        unsafe {
            (*out).value = 42;
        }
        0
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn opcl_da_write_i4(
        _: *mut c_void,
        _: *mut c_void,
        _: i32,
        result: *mut i32,
    ) -> i32 {
        // SAFETY: the wrapper passes a valid output pointer.
        unsafe {
            *result = 0;
        }
        0
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn opcl_da_read_scalar(
        _: *mut c_void,
        _: *mut c_void,
        max_age: u32,
        out: *mut ReadScalar,
        _: *mut c_char,
        _: u32,
    ) -> i32 {
        // SAFETY: the wrapper passes a valid output pointer.
        unsafe {
            (*out).vartype = 3;
            (*out).bits_low = if max_age == 0 {
                43
            } else if max_age == u32::MAX {
                42
            } else {
                44
            };
        }
        0
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn opcl_da_read_scalar_facts(
        session: *mut c_void,
        item: *mut c_void,
        max_age: u32,
        out: *mut NativeReadOutcome,
        text: *mut c_char,
        capacity: u32,
    ) -> i32 {
        if max_age == 0xabc001 {
            // SAFETY: out is provided by read_scalar and points to a writable result.
            unsafe {
                (*out).value.service_result = 0;
                (*out).value.item_result = 0;
                (*out).value.quality = 0xc0;
                (*out).value.vartype = 0x2003;
                (*out).value.filetime_low = 0xd53e8000;
                (*out).value.filetime_high = 0x019db1de;
                (*out).available = READ_SERVICE | READ_ITEM | READ_SAMPLE;
            }
            return -2147024774;
        }
        // SAFETY: the caller provides both output buffers for the duration of the call.
        let result = unsafe {
            opcl_da_read_scalar(session, item, max_age, &mut (*out).value, text, capacity)
        };
        // SAFETY: out is a live writable NativeReadOutcome as above.
        unsafe {
            (*out).available = READ_SERVICE | READ_ITEM | READ_SAMPLE;
        }
        result
    }
    #[cfg(feature = "native-da-test-hooks")]
    #[unsafe(no_mangle)]
    unsafe extern "C" fn opcl_da_test_mixed_read(
        _: *mut c_void,
        _: *mut c_void,
        _: *mut c_void,
        out: *mut NativeReadOutcome,
    ) -> i32 {
        // SAFETY: the wrapper supplies exactly two writable records.
        unsafe {
            (*out).available = READ_SERVICE | READ_ITEM | READ_SAMPLE;
            (*out).value.service_result = 1;
            (*out).value.item_result = 0;
            (*out).value.bits_low = 42;
            (*out.add(1)).available = READ_SERVICE | READ_ITEM;
            (*out.add(1)).value.service_result = 1;
            (*out.add(1)).value.item_result = 0x80070005_u32 as i32;
        }
        0
    }
    #[cfg(feature = "native-da-test-hooks")]
    #[test]
    fn mixed_read_preserves_service_and_item_availability() {
        let guid = Guid {
            data1: 1,
            data2: 2,
            data3: 3,
            data4: [0; 8],
        };
        let host = Host::new().unwrap();
        let shared = host.shared();
        let mut session = shared.open("server", "localhost", &guid).unwrap();
        let first = session.add("good").unwrap().0;
        let second = session.add("denied").unwrap().0;
        let values = session.test_mixed_read(first, second).unwrap();
        assert_eq!(values[0].available, READ_SERVICE | READ_ITEM | READ_SAMPLE);
        assert_eq!(values[0].value.service_result, 1);
        assert_eq!(values[0].value.bits_low, 42);
        assert_eq!(values[1].available, READ_SERVICE | READ_ITEM);
        assert_eq!(values[1].value.item_result, 0x80070005_u32 as i32);
        assert!(matches!(
            session.test_mixed_read(first, u64::MAX),
            Err(Error::InvalidHandle)
        ));
        session.close().unwrap();
        drop(shared);
        host.close().unwrap();
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn opcl_da_write_array(
        _: *mut c_void,
        _: *mut c_void,
        _: u16,
        _: *const u8,
        _: u32,
        result: *mut i32,
    ) -> i32 {
        // SAFETY: the wrapper passes a valid result pointer.
        unsafe {
            *result = 0;
        }
        0
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn opcl_da_write_text(
        _: *mut c_void,
        _: *mut c_void,
        _: *const c_char,
        result: *mut i32,
    ) -> i32 {
        // SAFETY: the wrapper passes a valid output pointer.
        unsafe {
            *result = 0;
        }
        0
    }
    #[cfg(feature = "native-da-test-hooks")]
    #[unsafe(no_mangle)]
    unsafe extern "C" fn opcl_da_test_async_prepare(_: *mut c_void) -> i32 {
        0
    }
    #[cfg(feature = "native-da-test-hooks")]
    static MOCK_ASYNC_TOKEN: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    #[cfg(feature = "native-da-test-hooks")]
    #[unsafe(no_mangle)]
    unsafe extern "C" fn opcl_da_test_async_read_value(
        _: *mut c_void,
        _: *mut c_void,
        age: u32,
        timeout: u32,
        out: *mut NativeReadOutcome,
        payload: *mut c_char,
        capacity: u32,
    ) -> i32 {
        MOCK_ASYNC_TOKEN.fetch_add(1, Ordering::SeqCst);
        if age == 123 || timeout == 100 {
            return 0x80070102_u32 as i32;
        }
        assert!(capacity >= 2);
        // SAFETY: mock receives the wrapper's live output buffers and writes two bytes.
        unsafe {
            (*out).available = READ_SERVICE | READ_ITEM | READ_SAMPLE;
            (*out).value.vartype = 8;
            (*out).value.quality = 0xc0;
            (*out).value.text_len = 2;
            std::ptr::copy_nonoverlapping(b"ok".as_ptr(), payload.cast::<u8>(), 2);
        }
        0
    }
    #[cfg(feature = "native-da-test-hooks")]
    #[unsafe(no_mangle)]
    unsafe extern "C" fn opcl_da_test_async_read_i4(
        _: *mut c_void,
        _: *mut c_void,
        _: u32,
        _: u32,
        _: *mut ReadI4,
    ) -> i32 {
        0x80070102_u32 as i32
    }
    #[cfg(feature = "native-da-test-hooks")]
    #[unsafe(no_mangle)]
    unsafe extern "C" fn opcl_da_test_async_wait_retire(
        _: *mut c_void,
        timeout: u32,
        out: *mut ReadI4,
        retired: *mut u32,
    ) -> i32 {
        if timeout == 0 {
            return 0x80070057_u32 as i32;
        }
        // SAFETY: the wrapper supplies a valid output for this mock call.
        unsafe {
            (*out).value = 0;
            *retired = 1;
        }
        0
    }
    #[cfg(feature = "native-da-test-hooks")]
    #[unsafe(no_mangle)]
    unsafe extern "C" fn opcl_da_test_async_last_token(_: *mut c_void, token: *mut u32) -> i32 {
        // SAFETY: the mock caller supplies a writable token.
        unsafe {
            *token = MOCK_ASYNC_TOKEN.load(Ordering::SeqCst);
        }
        0
    }
    #[cfg(feature = "native-da-test-hooks")]
    #[unsafe(no_mangle)]
    unsafe extern "C" fn opcl_da_test_async_is_pending(_: *mut c_void, pending: *mut u32) -> i32 {
        // SAFETY: mock caller supplies a valid writable output.
        unsafe {
            *pending = 1;
        }
        0
    }
    #[cfg(feature = "native-da-test-hooks")]
    #[test]
    fn generic_async_lease_copies_bstr_and_keeps_pending_fence() {
        let host = Host::new().unwrap();
        let shared = host.shared();
        let guid = Guid {
            data1: 1,
            data2: 2,
            data3: 3,
            data4: [0; 8],
        };
        let session = shared.open("server", "localhost", &guid).unwrap();
        drop(shared);
        let mut lease = TestAsyncSessionLease::new(session);
        lease.prepare().unwrap();
        let key = lease.session_mut().unwrap().add("item").unwrap().0;
        let raw = lease.read_value(key, 0, 10).unwrap();
        assert_eq!(raw.available, READ_SERVICE | READ_ITEM | READ_SAMPLE);
        assert_eq!(raw.value.vartype, 8);
        assert_eq!(raw.payload.unwrap().unwrap(), b"ok");
        assert!(lease.session_mut().is_none());
        assert!(matches!(lease.read_value(key, 0, 10), Err(Error::Busy)));
        assert_eq!(lease.wait_retire(10).unwrap().0, 0);
        let timed_out = lease.read_value(key, 123, 10).unwrap();
        assert_eq!(timed_out.adapter_result, 0x80070102_u32 as i32);
        assert_eq!(timed_out.available, 0);
        assert!(lease.session_mut().is_none());
        assert!(matches!(lease.read_value(key, 0, 10), Err(Error::Busy)));
        assert_eq!(lease.wait_retire(10).unwrap().0, 0);
        let session = match lease.into_session() {
            Ok(session) => session,
            Err(_) => panic!("still pending"),
        };
        session.close().unwrap();
        host.close().unwrap();
    }
    #[cfg(feature = "native-da-test-hooks")]
    #[test]
    fn test_async_lease_keeps_real_session_and_shared_host_until_retired() {
        let host = Host::new().unwrap();
        let shared = host.shared();
        let guid = Guid {
            data1: 1,
            data2: 2,
            data3: 3,
            data4: [0; 8],
        };
        let session = shared.open("server", "localhost", &guid).unwrap();
        drop(shared);
        let mut lease = TestAsyncSessionLease::new(session);
        lease.prepare().unwrap();
        let key = lease.session_mut().unwrap().add("item").unwrap().0;
        assert_eq!(lease.read(key, 0, 10).unwrap().0, 0x80070102_u32 as i32);
        assert!(!host.is_idle());
        assert!(lease.session_mut().is_none());
        let mut lease = match lease.into_session() {
            Err(lease) => lease,
            Ok(_) => panic!("pending lease escaped"),
        };
        assert_eq!(lease.wait_retire(0).unwrap().0, 0x80070057_u32 as i32);
        assert!(!host.is_idle());
        assert!(lease.session_mut().is_none());
        let mut lease = match lease.into_session() {
            Err(lease) => lease,
            Ok(_) => panic!("failed wait released pending lease"),
        };
        let (result, value) = lease.wait_retire(10).unwrap();
        assert_eq!(result, 0);
        assert_eq!(value.value, 0);
        assert!(lease.session_mut().is_some());
        let session = match lease.into_session() {
            Ok(session) => session,
            Err(_) => panic!("retired lease still pending"),
        };
        session.close().unwrap();
        assert!(host.is_idle());
        host.close().unwrap();
    }
    #[cfg(feature = "native-da-test-hooks")]
    #[unsafe(no_mangle)]
    unsafe extern "C" fn opcl_da_test_arm_write_exception() -> i32 {
        0
    }
    #[cfg(feature = "native-da-test-hooks")]
    #[unsafe(no_mangle)]
    unsafe extern "C" fn opcl_da_test_write_exception_state() -> i32 {
        0
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn opcl_da_write_scalar(
        _: *mut c_void,
        _: *mut c_void,
        _: u16,
        _: u32,
        _: u32,
        result: *mut i32,
    ) -> i32 {
        // SAFETY: the wrapper passes a valid output pointer.
        unsafe {
            *result = 0;
        }
        0
    }
    #[test]
    fn mock_ffi_failure_facts_survive_the_rust_bridge() {
        let guid = Guid {
            data1: 1,
            data2: 2,
            data3: 3,
            data4: [0; 8],
        };
        let host = Host::new().unwrap();
        let shared = host.shared();
        let mut session = shared.open("server", "localhost", &guid).unwrap();
        let key = session.add("item").unwrap().0;
        let outcome = session.read_scalar(key, 0xabc001).unwrap();
        assert_eq!(outcome.adapter_result, -2147024774);
        assert_eq!(outcome.available, READ_SERVICE | READ_ITEM | READ_SAMPLE);
        assert_eq!(outcome.value.quality, 0xc0);
        assert_eq!(outcome.value.service_result, 0);
        assert_eq!(outcome.value.item_result, 0);
        assert_eq!(outcome.value.vartype, 0x2003);
        assert_eq!(outcome.value.filetime_low, 0xd53e8000);
        assert_eq!(outcome.value.filetime_high, 0x019db1de);
        assert!(matches!(outcome.payload, Ok(None)));
        session.close().unwrap();
        drop(shared);
        host.close().unwrap();
    }

    #[test]
    fn stale_and_foreign_registrations_do_not_reach_native_read() {
        assert_eq!(std::mem::size_of::<ReadScalar>(), 32);
        assert_eq!(std::mem::size_of::<NativeReadOutcome>(), 36);
        assert_eq!(std::mem::offset_of!(NativeReadOutcome, available), 32);
        assert_eq!(std::mem::size_of::<PushRecord>(), 48);
        assert_eq!(std::mem::offset_of!(PushRecord, value), 16);
        assert_eq!(std::mem::size_of::<BrowseRecord>(), 5132);
        assert_eq!(std::mem::offset_of!(BrowseRecord, name), 12);
        assert_eq!(std::mem::offset_of!(BrowseRecord, node_id), 1036);
        assert_eq!(std::mem::size_of::<ServerRecord>(), 3100);
        assert_eq!(std::mem::offset_of!(ServerRecord, progid), 28);
        assert_eq!(std::mem::offset_of!(ServerRecord, description), 1052);
        assert_eq!(std::mem::offset_of!(ReadScalar, bits_low), 16);
        assert_eq!(std::mem::offset_of!(ReadScalar, quality), 24);
        assert_eq!(std::mem::offset_of!(ReadScalar, vartype), 26);
        let guid = Guid {
            data1: 1,
            data2: 2,
            data3: 3,
            data4: [0; 8],
        };
        let host = Host::new().unwrap();
        let shared = host.shared();
        let mut a = shared.open("server", "localhost", &guid).unwrap();
        let mut b = shared.open("server", "localhost", &guid).unwrap();
        let old = a.add("item").unwrap().0;
        let foreign = b.add("item").unwrap().0;
        assert_ne!(old, foreign);
        assert!(matches!(a.read_i4(foreign, 0), Err(Error::InvalidHandle)));
        a.remove(old).unwrap();
        let fresh = a.add("item").unwrap().0;
        assert_ne!(old, fresh);
        assert!(matches!(a.read_i4(old, 0), Err(Error::InvalidHandle)));
        assert_eq!(a.read_i4(fresh, 0).unwrap().value, 42);
        assert!(matches!(a.write_i4(old, 7), Err(Error::InvalidHandle)));
        assert_eq!(a.write_i4(fresh, 7).unwrap(), 0);
        let unicode = a.add("Fixture.Тег").unwrap().0;
        assert_eq!(a.write_text(unicode, "РУГ 中文").unwrap(), 0);
        assert_eq!(
            a.write_text(unicode, "bad\0string"),
            Err(Error::InvalidString)
        );
        a.close().unwrap();
        b.close().unwrap();
        drop(shared);
        host.close().unwrap();

        let host = Host::new().unwrap();
        let shared = host.shared();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let mut session = shared.open("server", "localhost", &guid).unwrap();
            ready_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            let key = session.add("item").unwrap().0;
            assert_eq!(session.read_i4(key, 0).unwrap().value, 42);
            session.close().unwrap();
        });
        ready_rx.recv().unwrap();
        // A detached worker retains the native host; closing it reports Busy.
        assert_eq!(host.close(), Err(Error::Busy));
        release_tx.send(()).unwrap();
        worker.join().unwrap();
    }
}
