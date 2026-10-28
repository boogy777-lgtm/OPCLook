//! Ownership for values allocated by COM's task allocator.
#![deny(clippy::undocumented_unsafe_blocks)]

use std::ffi::c_void;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::core::PWSTR;

/// A nullable task-allocated UTF-16 string.
pub(super) struct CoString(PWSTR);

impl CoString {
    /// # Safety
    /// A non-null pointer must uniquely own a NUL-terminated task allocation.
    pub(super) unsafe fn from_raw(ptr: PWSTR) -> Option<Self> {
        if ptr.is_null() { None } else { Some(Self(ptr)) }
    }

    pub(super) fn text(&self) -> String {
        // SAFETY: construction requires a valid NUL-terminated COM string.
        unsafe { self.0.to_string().unwrap_or_default() }
    }
}

impl Drop for CoString {
    fn drop(&mut self) {
        // SAFETY: this guard uniquely owns a non-null task allocation.
        unsafe { CoTaskMemFree(Some(self.0.0.cast::<c_void>())) };
    }
}

/// A nullable allocation whose elements require no individual destruction.
pub(super) struct CoAllocation<T>(*mut T);

impl<T> CoAllocation<T> {
    /// # Safety
    /// The pointer must be null or a uniquely owned task allocation.
    pub(super) unsafe fn from_raw(ptr: *mut T) -> Self {
        Self(ptr)
    }

    pub(super) fn as_ptr(&self) -> *mut T {
        self.0
    }
}

impl<T> Drop for CoAllocation<T> {
    fn drop(&mut self) {
        // SAFETY: callers transfer a unique task allocation or a null pointer.
        unsafe { CoTaskMemFree(Some(self.0.cast::<c_void>())) };
    }
}
