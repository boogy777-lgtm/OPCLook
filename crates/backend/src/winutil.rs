//! Small ownership helpers for Windows APIs.
#![deny(clippy::undocumented_unsafe_blocks)]

use std::ffi::CStr;
use windows::Win32::Foundation::{CloseHandle, FreeLibrary, HANDLE, HMODULE};
use windows::Win32::System::LibraryLoader::{
    GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
};
use windows::core::{PCSTR, PCWSTR, PWSTR};

/// One closeable Windows handle. The integer stores the opaque handle value,
/// so ownership can move between threads without exposing a borrowed pointer.
#[derive(Debug)]
pub(crate) struct OwnedHandle(usize);

impl OwnedHandle {
    /// # Safety
    /// The handle must be valid, closeable, and uniquely owned by the caller.
    pub(crate) unsafe fn from_raw(handle: HANDLE) -> Self {
        Self(handle.0 as usize)
    }

    pub(crate) fn raw(&self) -> HANDLE {
        HANDLE(self.0 as *mut core::ffi::c_void)
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: construction transferred the sole close obligation.
        unsafe {
            let _ = CloseHandle(self.raw());
        }
    }
}

/// Owns a table returned by an IP Helper API.
pub(crate) struct MibTable<T>(*mut T);
impl<T> MibTable<T> {
    /// # Safety
    /// `ptr` must be a uniquely owned, non-null IP Helper table allocation.
    pub(crate) unsafe fn from_raw(ptr: *mut T) -> Self {
        Self(ptr)
    }
    pub(crate) fn as_ptr(&self) -> *mut T {
        self.0
    }
}
impl<T> Drop for MibTable<T> {
    fn drop(&mut self) {
        // SAFETY: construction transfers the sole FreeMibTable obligation.
        unsafe { windows::Win32::NetworkManagement::IpHelper::FreeMibTable(self.0.cast()) };
    }
}

/// A loaded system DLL, retained for at least as long as any resolved symbol.
pub(crate) struct SystemLibrary(usize);

impl SystemLibrary {
    pub(crate) fn load(name: &str) -> Option<Self> {
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: wide is NUL-terminated and remains alive for this call.
        let module =
            unsafe { LoadLibraryExW(PCWSTR(wide.as_ptr()), None, LOAD_LIBRARY_SEARCH_SYSTEM32) }
                .ok()?;
        Some(Self(module.0 as usize))
    }

    /// # Safety
    /// `F` must be the exact function-pointer type exported under `name`.
    /// The returned pointer must not outlive this library.
    pub(crate) unsafe fn symbol<F: Copy>(&self, name: &CStr) -> Option<F> {
        let module = HMODULE(self.0 as *mut core::ffi::c_void);
        // SAFETY: the module is held loaded and name is NUL-terminated.
        let symbol = unsafe { GetProcAddress(module, PCSTR(name.as_ptr().cast())) }?;
        if std::mem::size_of::<F>() != std::mem::size_of_val(&symbol) {
            return None;
        }
        // SAFETY: the caller supplies the exact exported function type and the
        // size check establishes the representations have the same size.
        Some(unsafe { std::mem::transmute_copy(&symbol) })
    }
}

impl Drop for SystemLibrary {
    fn drop(&mut self) {
        // SAFETY: load acquired this reference and no symbol outlives the guard.
        unsafe {
            let _ = FreeLibrary(HMODULE(self.0 as *mut core::ffi::c_void));
        }
    }
}

/// # Safety
/// A non-null pointer must address a readable, NUL-terminated UTF-16 string.
pub(crate) unsafe fn wide_to_string(value: PWSTR) -> String {
    if value.is_null() {
        return String::new();
    }
    let mut len = 0usize;
    // SAFETY: the caller guarantees a readable NUL-terminated string.
    unsafe {
        while *value.0.add(len) != 0 {
            len += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(value.0, len))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_handle_closes_during_unwind() {
        use windows::Win32::Foundation::GetHandleInformation;
        use windows::Win32::System::Threading::CreateEventW;
        // SAFETY: null attributes and name create an unnamed event.
        let raw = unsafe { CreateEventW(None, false, false, PCWSTR::null()) }.unwrap();
        let outcome = std::panic::catch_unwind(|| {
            // SAFETY: the newly created event is transferred exactly once.
            let _owned = unsafe { OwnedHandle::from_raw(raw) };
            panic!("exercise handle cleanup during unwind");
        });
        assert!(outcome.is_err());
        let mut flags = 0;
        // SAFETY: querying the closed handle value reports an invalid handle.
        assert!(unsafe { GetHandleInformation(raw, &mut flags) }.is_err());
    }

    #[test]
    fn system_library_resolves_typed_symbol_and_missing_export() {
        let library = SystemLibrary::load("kernel32.dll").unwrap();
        type ProcessId = unsafe extern "system" fn() -> u32;
        // SAFETY: the export's signature is GetCurrentProcessId(void).
        let process_id = unsafe { library.symbol::<ProcessId>(c"GetCurrentProcessId") }.unwrap();
        // SAFETY: this function takes no arguments and has no preconditions.
        assert_eq!(unsafe { process_id() }, std::process::id());
        // SAFETY: a missing symbol is never called; this is the expected signature.
        assert!(unsafe { library.symbol::<ProcessId>(c"OPCLookMissingExport") }.is_none());
    }

    #[test]
    fn wide_conversion_accepts_null_and_unicode() {
        // SAFETY: null is accepted and does not require readable memory.
        assert_eq!(unsafe { wide_to_string(PWSTR::null()) }, "");
        let text = [0x4e2du16, 0x6587, 0];
        // SAFETY: the stack array is readable and NUL-terminated.
        let converted = unsafe { wide_to_string(PWSTR(text.as_ptr().cast_mut())) };
        assert_eq!(converted, "中文");
    }
}
