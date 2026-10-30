extern crate self as tokio;
#[cfg(test)]
pub use opclook_locale as i18n;
pub mod sync {
    pub mod mpsc {
        pub mod error { pub use std::sync::mpsc::TryRecvError; }
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
            mpsc,
        };
        pub struct UnboundedSender<T> {
            inner: mpsc::Sender<T>,
            closed: Arc<AtomicBool>,
        }
        pub struct UnboundedReceiver<T> {
            inner: mpsc::Receiver<T>,
            closed: Arc<AtomicBool>,
        }
        pub fn unbounded_channel<T>() -> (UnboundedSender<T>, UnboundedReceiver<T>) {
            let (tx, rx) = mpsc::channel();
            let closed = Arc::new(AtomicBool::new(false));
            (
                UnboundedSender {
                    inner: tx,
                    closed: closed.clone(),
                },
                UnboundedReceiver { inner: rx, closed },
            )
        }
        impl<T> UnboundedSender<T> {
            pub fn send(&self, v: T) -> Result<(), mpsc::SendError<T>> {
                if self.closed.load(Ordering::Relaxed) {
                    Err(mpsc::SendError(v))
                } else {
                    self.inner.send(v)
                }
            }
            pub fn is_closed(&self) -> bool {
                self.closed.load(Ordering::Relaxed)
            }
        }
        impl<T> UnboundedReceiver<T> {
            pub fn blocking_recv(&mut self) -> Option<T> {
                self.inner.recv().ok()
            }
            pub fn close(&mut self) {
                self.closed.store(true, Ordering::Relaxed);
            }
            pub fn try_recv(&mut self) -> Result<T, mpsc::TryRecvError> {
                self.inner.try_recv()
            }
        }
        impl<T> Drop for UnboundedReceiver<T> {
            fn drop(&mut self) {
                self.closed.store(true, Ordering::Relaxed);
            }
        }
    }
}
pub mod bff;
