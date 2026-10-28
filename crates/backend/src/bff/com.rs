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

    pub(super) fn try_text(&self) -> Option<String> {
        // SAFETY: construction requires a valid NUL-terminated COM string.
        unsafe { self.0.to_string().ok() }
    }

    pub(super) fn text(&self) -> String {
        self.try_text().unwrap_or_default()
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

/// A task-allocated array with per-element COM resources.
pub(super) struct ComArray<T> {
    allocation: CoAllocation<T>,
    len: usize,
    clear: unsafe fn(&mut T),
}

impl<T> ComArray<T> {
    /// # Safety
    /// `ptr` must be null or uniquely own `len` initialized elements in a task
    /// allocation. `clear` must release exactly the resources owned by one element.
    pub(super) unsafe fn from_raw(ptr: *mut T, len: usize, clear: unsafe fn(&mut T)) -> Self {
        Self {
            // SAFETY: ownership is transferred by the caller.
            allocation: unsafe { CoAllocation::from_raw(ptr) },
            len,
            clear,
        }
    }

    pub(super) fn as_ptr(&self) -> *mut T {
        self.allocation.as_ptr()
    }
}

impl<T> Drop for ComArray<T> {
    fn drop(&mut self) {
        let ptr = self.allocation.as_ptr();
        if !ptr.is_null() {
            for index in 0..self.len {
                // SAFETY: construction guarantees all len elements are initialized
                // and uniquely owned; each is cleared exactly once before freeing.
                unsafe { (self.clear)(&mut *ptr.add(index)) };
            }
        }
    }
}

/// Owns one initialized VARIANT, including any nested COM resources.
pub(super) struct OwnedVariant(pub(super) windows::Win32::System::Variant::VARIANT);

impl Drop for OwnedVariant {
    fn drop(&mut self) {
        // SAFETY: the guard uniquely owns a valid initialized VARIANT.
        unsafe {
            let _ = windows::Win32::System::Variant::VariantClear(&mut self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use windows::Win32::System::Com::CoTaskMemAlloc;

    unsafe fn count_release(item: &mut *const AtomicUsize) {
        // SAFETY: the test keeps its counter alive until the array is dropped.
        unsafe { (**item).fetch_add(1, Ordering::SeqCst) };
    }

    #[test]
    fn array_clears_each_element_on_error_and_unwind() {
        for unwind in [false, true] {
            let count = AtomicUsize::new(0);
            let run = || -> Result<(), ()> {
                let len = 3;
                // SAFETY: allocation size is sufficient for three aligned pointers.
                let ptr =
                    unsafe { CoTaskMemAlloc(len * std::mem::size_of::<*const AtomicUsize>()) }
                        .cast::<*const AtomicUsize>();
                assert!(!ptr.is_null());
                for index in 0..len {
                    // SAFETY: ptr owns len elements, each initialized exactly once.
                    unsafe { ptr.add(index).write(&count) };
                }
                // SAFETY: all elements are initialized and refer to the live counter.
                let _array = unsafe { ComArray::from_raw(ptr, len, count_release) };
                if unwind {
                    panic!("exercise cleanup during unwinding");
                }
                Err(())
            };
            let outcome = std::panic::catch_unwind(run);
            assert_eq!(outcome.is_err(), unwind);
            assert_eq!(count.load(Ordering::SeqCst), 3);
        }
    }
}
