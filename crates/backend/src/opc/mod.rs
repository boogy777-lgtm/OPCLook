// COM interface bindings: Windows only (nothing else uses them).
#[cfg(windows)]
pub mod bindings {
    #[allow(clippy::all)]
    pub mod comn;
    #[allow(clippy::all)]
    pub mod da;
}
