//! Bounded, fixed-size DNS pool for route preparation.
use opclook_contract::da_route::DaRouteError;
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Instant;

const WORKERS: usize = 2;
const QUEUE: usize = 16;
type Resolver = dyn Fn(&str) -> Result<Vec<IpAddr>, DaRouteError> + Send + Sync + 'static;
struct Job {
    host: String,
    deadline: Instant,
    cancelled: Arc<AtomicBool>,
    result: mpsc::SyncSender<Result<Vec<IpAddr>, DaRouteError>>,
}

/// A submitted DNS request. Dropping it cancels delivery of any late result.
pub struct ResolveTicket {
    cancelled: Arc<AtomicBool>,
    result: mpsc::Receiver<Result<Vec<IpAddr>, DaRouteError>>,
    deadline: Instant,
}
impl ResolveTicket {
    pub fn poll(&self) -> Option<Result<Vec<IpAddr>, DaRouteError>> {
        match self.result.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) if Instant::now() < self.deadline => None,
            Err(_) => Some(Err(DaRouteError::Resolve(
                "resolution timed out or stopped".into(),
            ))),
        }
    }
}
impl Drop for ResolveTicket {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

/// Exactly two workers. DNS may block indefinitely, but no request starts a
/// new worker; submissions fail immediately when the bounded queue is full.
pub struct ResolverPool {
    jobs: mpsc::SyncSender<Job>,
}
impl ResolverPool {
    pub fn new() -> std::io::Result<Self> {
        Self::with_resolver(Arc::new(crate::da_route::resolve_targets))
    }
    fn with_resolver(resolve: Arc<Resolver>) -> std::io::Result<Self> {
        let (tx, rx) = mpsc::sync_channel::<Job>(QUEUE);
        let rx = Arc::new(Mutex::new(rx));
        for index in 0..WORKERS {
            let rx = Arc::clone(&rx);
            let resolve = Arc::clone(&resolve);
            std::thread::Builder::new()
                .name(format!("da-resolver-{index}"))
                .spawn(move || {
                    loop {
                        let job = rx
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .recv();
                        let Ok(job) = job else {
                            break;
                        };
                        if job.cancelled.load(Ordering::Acquire) || Instant::now() >= job.deadline {
                            continue;
                        }
                        let result = resolve(&job.host);
                        if !job.cancelled.load(Ordering::Acquire) && Instant::now() < job.deadline {
                            let _ = job.result.try_send(result);
                        }
                    }
                })?;
        }
        Ok(Self { jobs: tx })
    }
    pub fn submit(&self, host: String, deadline: Instant) -> Result<ResolveTicket, DaRouteError> {
        let cancelled = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::sync_channel(1);
        let job = Job {
            host,
            deadline,
            cancelled: Arc::clone(&cancelled),
            result: tx,
        };
        self.jobs
            .try_send(job)
            .map_err(|_| DaRouteError::Resolve("resolver busy or stopped".into()))?;
        Ok(ResolveTicket {
            cancelled,
            result: rx,
            deadline,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    #[test]
    fn bounded_workers_reject_excess_and_cancel_late_results() {
        let started = Arc::new(AtomicUsize::new(0));
        let gate = Arc::new(AtomicBool::new(false));
        struct OpenGate(Arc<AtomicBool>);
        impl Drop for OpenGate {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let _open_gate = OpenGate(Arc::clone(&gate));
        let resolver = {
            let started = Arc::clone(&started);
            let gate = Arc::clone(&gate);
            Arc::new(move |_: &str| {
                started.fetch_add(1, Ordering::SeqCst);
                while !gate.load(Ordering::SeqCst) {
                    std::thread::yield_now();
                }
                Ok(vec!["127.0.0.1".parse().unwrap()])
            })
        };
        let pool = ResolverPool::with_resolver(resolver).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        let first = pool.submit("one".into(), deadline).unwrap();
        let second = pool.submit("two".into(), deadline).unwrap();
        let wait = Instant::now() + Duration::from_secs(1);
        while started.load(Ordering::SeqCst) < 2 && Instant::now() < wait {
            std::thread::yield_now();
        }
        assert_eq!(started.load(Ordering::SeqCst), 2);
        let queued: Vec<_> = (0..QUEUE)
            .map(|_| pool.submit("queued".into(), deadline).unwrap())
            .collect();
        assert!(pool.submit("excess".into(), deadline).is_err());
        drop(first);
        drop(queued);
        gate.store(true, Ordering::SeqCst);
        let wait = Instant::now() + Duration::from_secs(1);
        let result = loop {
            if let Some(result) = second.poll() {
                break Some(result);
            }
            if Instant::now() >= wait {
                break None;
            }
            std::thread::yield_now();
        };
        assert!(
            matches!(result, Some(Ok(addresses)) if addresses == vec!["127.0.0.1".parse::<IpAddr>().unwrap()])
        );
        assert_eq!(started.load(Ordering::SeqCst), 2);
    }
}
