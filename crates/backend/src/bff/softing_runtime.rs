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
        Self { is_branch: 0, name_len: 0, node_id_len: 0, name: [0; 1024], node_id: [0; 4096] }
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
        Self { clsid: Guid::default(), progid_len: 0, description_len: 0, flags: 0,
               progid: [0; 1024], description: [0; 2048] }
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
#[derive(Default, Debug)]
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
unsafe extern "C" {
    fn opcl_da_browse(session: *mut c_void, node_id: *const c_char, out: *mut BrowseRecord, capacity: u32, count: *mut u32) -> i32;
    fn opcl_da_resolve(host: *mut c_void, node: *const c_char, progid: *const c_char, kind: u32, out: *mut Guid) -> i32;
    fn opcl_da_discover(host: *mut c_void, node: *const c_char, out: *mut ServerRecord, capacity: u32, count: *mut u32, fallback: *mut u32) -> i32;
    fn opcl_da_host_create(out: *mut *mut c_void) -> i32;
    fn opcl_da_host_destroy(host: *mut c_void) -> i32;
    fn opcl_da_session_open(
        host: *mut c_void,
        name: *const c_char,
        node: *const c_char,
        clsid: *const Guid,
        out: *mut *mut c_void,
    ) -> i32;
    fn opcl_da_session_close(session: *mut c_void) -> i32;
    fn opcl_da_item_add(
        session: *mut c_void,
        id: *const c_char,
        out: *mut *mut c_void,
        canonical_type: *mut u16,
        rights: *mut u32,
    ) -> i32;
    fn opcl_da_item_remove(session: *mut c_void, item: *mut c_void) -> i32;
    fn opcl_da_read_i4(session: *mut c_void, item: *mut c_void, age: u32, out: *mut ReadI4) -> i32;
    fn opcl_da_read_scalar(
        session: *mut c_void,
        item: *mut c_void,
        age: u32,
        out: *mut ReadScalar,
        text: *mut c_char,
        text_capacity: u32,
    ) -> i32;
    fn opcl_da_write_text(session: *mut c_void, item: *mut c_void, text: *const c_char, result: *mut i32) -> i32;
    fn opcl_da_write_scalar(
        session: *mut c_void,
        item: *mut c_void,
        vt: u16,
        low: u32,
        high: u32,
        result: *mut i32,
    ) -> i32;
    fn opcl_da_write_i4(
        session: *mut c_void,
        item: *mut c_void,
        value: i32,
        result: *mut i32,
    ) -> i32;
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
    #[cfg(test)]
    pub fn same_host(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.core, &other.core)
    }
    pub fn resolve(&self, node: &str, progid: &str, remote: bool) -> Result<Guid, Error> {
        let node = string(node)?; let progid = string(progid)?; let mut out = Guid::default();
        // SAFETY: the shared host remains live and the inputs and output live through this synchronous call.
        check(unsafe { opcl_da_resolve(self.core.ptr.as_ptr(), node.as_ptr(), progid.as_ptr(), u32::from(remote), &mut out) })?;
        Ok(out)
    }
    pub fn discover(&self, node: &str) -> Result<(Vec<DiscoveredServer>, bool), Error> {
        let node = string(node)?;
        let mut records = vec![ServerRecord::default(); 256];
        let mut count = 0; let mut fallback = 0;
        // SAFETY: the host is live, records owns capacity entries, and every output remains valid for the call.
        check(unsafe { opcl_da_discover(self.core.ptr.as_ptr(), node.as_ptr(), records.as_mut_ptr(), 256, &mut count, &mut fallback) })?;
        let count = usize::try_from(count).map_err(|_| Error::InvalidString)?;
        if count > records.len() || fallback > 1 { return Err(Error::InvalidString); }
        let mut found = Vec::with_capacity(count);
        for record in records.into_iter().take(count) {
            let progid = record.progid.get(..record.progid_len as usize).ok_or(Error::InvalidString)?;
            let description = record.description.get(..record.description_len as usize).ok_or(Error::InvalidString)?;
            found.push(DiscoveredServer {
                clsid: record.clsid,
                progid: String::from_utf8(progid.to_vec()).map_err(|_| Error::InvalidString)?,
                description: String::from_utf8(description.to_vec()).map_err(|_| Error::InvalidString)?,
                description_missing: record.flags & 1 != 0,
            });
        }
        Ok((found, fallback != 0))
    }
    pub fn open(&self, name: &str, node: &str, clsid: &Guid) -> Result<Session, Error> {
        let name = string(name)?;
        let node = string(node)?;
        let mut ptr = std::ptr::null_mut();
        // SAFETY: self retains the live host, C strings and outputs live through the call.
        check(unsafe {
            opcl_da_session_open(
                self.core.ptr.as_ptr(),
                name.as_ptr(),
                node.as_ptr(),
                clsid,
                &mut ptr,
            )
        })?;
        Ok(Session {
            ptr: NonNull::new(ptr).ok_or(Error::NullHandle)?,
            _host: self.clone(),
            items: HashMap::new(),
            closed: false,
            _thread: PhantomData,
        })
    }
}
pub struct Session {
    ptr: NonNull<c_void>,
    _host: SharedHost,
    items: HashMap<u64, NonNull<c_void>>,
    closed: bool,
    _thread: PhantomData<Rc<()>>,
}
impl Session {
    pub fn browse(&mut self, node_id: &str) -> Result<Vec<BrowseEntry>, Error> {
        let node_id = string(node_id)?;
        let mut records = vec![BrowseRecord::default(); 256]; let mut count = 0;
        // SAFETY: the session is uniquely owned and the input/output buffers live until this synchronous call returns.
        check(unsafe { opcl_da_browse(self.ptr.as_ptr(), node_id.as_ptr(), records.as_mut_ptr(), 256, &mut count) })?;
        let count = usize::try_from(count).map_err(|_| Error::InvalidString)?;
        if count > records.len() { return Err(Error::InvalidString); }
        let mut result = Vec::with_capacity(count);
        for record in records.into_iter().take(count) {
            if record.is_branch > 1 { return Err(Error::InvalidString); }
            let name = record.name.get(..record.name_len as usize).ok_or(Error::InvalidString)?;
            let id = record.node_id.get(..record.node_id_len as usize).ok_or(Error::InvalidString)?;
            result.push(BrowseEntry { name: String::from_utf8(name.to_vec()).map_err(|_| Error::InvalidString)?,
                node_id: String::from_utf8(id.to_vec()).map_err(|_| Error::InvalidString)?,
                is_branch: record.is_branch == 1 });
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
    pub fn read_scalar(&mut self, key: u64, max_age: u32) -> Result<(ReadScalar, Option<String>), Error> {
        let item = self.items.get(&key).ok_or(Error::InvalidHandle)?;
        let mut out = ReadScalar::default();
        let mut text = vec![0u8; 65_536];
        // SAFETY: the map owns the registration and both buffers remain valid for the call.
        check(unsafe { opcl_da_read_scalar(self.ptr.as_ptr(), item.as_ptr(), max_age, &mut out, text.as_mut_ptr().cast(), text.len() as u32) })?;
        let text = if out.vartype == 8 {
            let count = usize::try_from(out.text_len).map_err(|_| Error::InvalidString)?;
            if count > text.len() { return Err(Error::InvalidString); }
            text.truncate(count);
            Some(String::from_utf8(text).map_err(|_| Error::InvalidString)?)
        } else { None };
        Ok((out, text))
    }
    pub fn write_text(&mut self, key: u64, value: &str) -> Result<i32, Error> {
        let item = self.items.get(&key).ok_or(Error::InvalidHandle)?;
        let text = string(value)?;
        let mut result = 0;
        // SAFETY: the registration and the C string and output outlive the call.
        check(unsafe { opcl_da_write_text(self.ptr.as_ptr(), item.as_ptr(), text.as_ptr(), &mut result) })?;
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
    use std::sync::atomic::{AtomicUsize, Ordering};
    static HANDLE: AtomicUsize = AtomicUsize::new(100);
    fn handle() -> *mut c_void {
        HANDLE.fetch_add(1, Ordering::Relaxed) as *mut c_void
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn opcl_da_browse(_: *mut c_void, path: *const c_char, out: *mut BrowseRecord, capacity: u32, count: *mut u32) -> i32 {
        // SAFETY: the wrapper supplies a valid NUL-terminated string and writable output pointers.
        unsafe {
            if capacity == 0 { return -1; }
            let name = if std::ffi::CStr::from_ptr(path).to_bytes().is_empty() { b"Fixture".as_slice() } else { b"Value".as_slice() };
            let id = if name == b"Fixture" { b"Fixture".as_slice() } else { b"Fixture.Value".as_slice() };
            let record = &mut *out;
            record.name[..name.len()].copy_from_slice(name);
            record.node_id[..id.len()].copy_from_slice(id);
            record.name_len = name.len() as u32; record.node_id_len = id.len() as u32;
            record.is_branch = u32::from(name == b"Fixture"); *count = 1;
        }
        0
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn opcl_da_resolve(_: *mut c_void, _: *const c_char, _: *const c_char, _: u32, out: *mut Guid) -> i32 {
        // SAFETY: the wrapper supplies a valid output pointer.
        unsafe { *out = Guid { data1: 1, ..Guid::default() }; }
        0
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn opcl_da_discover(_: *mut c_void, _: *const c_char, out: *mut ServerRecord, capacity: u32, count: *mut u32, fallback: *mut u32) -> i32 {
        // SAFETY: the wrapper supplies at least one record and valid output pointers.
        unsafe {
            if capacity == 0 { return -1; }
            let record = &mut *out;
            let id = b"OPCLook.NativeDaFixture.1";
            record.progid[..id.len()].copy_from_slice(id);
            record.progid_len = id.len() as u32;
            record.clsid = Guid { data1: 1, ..Guid::default() };
            *count = 1; *fallback = 1;
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
        _: *const Guid,
        out: *mut *mut c_void,
    ) -> i32 {
        // SAFETY: the wrapper passes a valid out pointer.
        unsafe { *out = handle() };
        0
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn opcl_da_session_close(_: *mut c_void) -> i32 {
        0
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn opcl_da_item_add(
        _: *mut c_void,
        _: *const c_char,
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
        _: u32,
        out: *mut ReadScalar,
        _: *mut c_char,
        _: u32,
    ) -> i32 {
        // SAFETY: the wrapper passes a valid output pointer.
        unsafe {
            (*out).vartype = 3;
            (*out).bits_low = 42;
        }
        0
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn opcl_da_write_text(
        _: *mut c_void, _: *mut c_void, _: *const c_char, result: *mut i32,
    ) -> i32 {
        // SAFETY: the wrapper passes a valid output pointer.
        unsafe { *result = 0; }
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
    fn stale_and_foreign_registrations_do_not_reach_native_read() {
        assert_eq!(std::mem::size_of::<ReadScalar>(), 32);
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
        assert_eq!(a.write_text(unicode, "bad\0string"), Err(Error::InvalidString));
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
