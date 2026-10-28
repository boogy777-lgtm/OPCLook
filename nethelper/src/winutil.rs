//! Owned handles and local allocations used by the privileged helper.
#![deny(clippy::undocumented_unsafe_blocks)]

use windows::Win32::Foundation::{CloseHandle, HANDLE, HLOCAL, LocalFree};
use windows::Win32::Security::RevertToSelf;

pub(super) struct OwnedHandle(HANDLE);
impl OwnedHandle {
    /// # Safety
    /// The caller transfers one valid, uniquely owned closeable handle.
    pub(super) unsafe fn from_raw(handle: HANDLE) -> Self {
        Self(handle)
    }
    pub(super) fn raw(&self) -> HANDLE {
        self.0
    }
}
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: construction transferred a valid handle and this is its sole owner.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub(super) struct LocalAllocation(HLOCAL);
impl LocalAllocation {
    /// # Safety
    /// The pointer must be null or a uniquely owned LocalAlloc-compatible allocation.
    pub(super) unsafe fn from_raw(ptr: *mut core::ffi::c_void) -> Self {
        Self(HLOCAL(ptr))
    }
    pub(super) fn as_ptr(&self) -> *mut core::ffi::c_void {
        self.0.0
    }
}
impl Drop for LocalAllocation {
    fn drop(&mut self) {
        // SAFETY: this guard owns the local allocation, or holds null.
        unsafe {
            let _ = LocalFree(Some(self.0));
        }
    }
}

/// Balances successful impersonation even if the following work unwinds.
pub(super) struct Impersonation(std::marker::PhantomData<std::rc::Rc<()>>);
impl Impersonation {
    /// # Safety
    /// The current thread must already be impersonating and own that state.
    pub(super) unsafe fn from_current_thread() -> Self {
        Self(std::marker::PhantomData)
    }
}
impl Drop for Impersonation {
    fn drop(&mut self) {
        // SAFETY: the guard is created only after successful impersonation on this thread.
        unsafe {
            let _ = RevertToSelf();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Foundation::GetHandleInformation;
    use windows::Win32::System::Threading::CreateEventW;
    use windows::core::PCWSTR;

    #[test]
    fn handle_closes_on_early_return() {
        // SAFETY: null attributes/name request an unnamed ordinary event.
        let raw = unsafe { CreateEventW(None, false, false, PCWSTR::null()) }.unwrap();
        let early = || -> Result<(), ()> {
            // SAFETY: ownership of the newly created handle is transferred once.
            let _owned = unsafe { OwnedHandle::from_raw(raw) };
            Err(())
        };
        assert!(early().is_err());
        let mut flags = 0;
        // SAFETY: the raw value can be queried after close; the API reports invalid handle.
        assert!(unsafe { GetHandleInformation(raw, &mut flags) }.is_err());
    }
}
