//! Bounded route preparation and lease ownership off the frontend thread.
use crate::da_route::{DaRouteManager, PreparedRoutes};
use crate::net::AdapterInfo;
use crate::route_resolver::{ResolveTicket, ResolverPool};
use opclook_contract::da_route::{DaRouteError, DaRouteStatus};
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

const CAPACITY: usize = 32;
struct State {
    cancelled: AtomicBool,
    result: Mutex<Option<Result<(), DaRouteError>>>,
    permit_live: AtomicBool,
    permits: Arc<AtomicUsize>,
}
impl State {
    fn release_permit(&self) {
        if self.permit_live.swap(false, Ordering::AcqRel) {
            self.permits.fetch_sub(1, Ordering::AcqRel);
        }
    }
    fn finish(&self, result: Result<(), DaRouteError>) {
        *self
            .result
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(result);
        self.release_permit();
    }
}
/// A preparation and, after success, its route lease. Dropping it signals
/// cancellation/release directly; this never depends on enqueueing a message.
pub struct RouteTicket {
    state: Arc<State>,
}
impl RouteTicket {
    pub fn result(&self) -> Option<Result<(), DaRouteError>> {
        self.state
            .result
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}
impl Drop for RouteTicket {
    fn drop(&mut self) {
        self.state.cancelled.store(true, Ordering::Release);
        self.state.release_permit();
    }
}
struct Job {
    host: String,
    adapter: Option<AdapterInfo>,
    local: Vec<IpAddr>,
    deadline: Instant,
    state: Arc<State>,
}
struct Pending {
    job: Job,
    dns: ResolveTicket,
}
struct Held {
    state: Arc<State>,
    lease: PreparedRoutes,
}

pub struct RouteService {
    jobs: mpsc::SyncSender<Job>,
    stop: Arc<AtomicBool>,
    permits: Arc<AtomicUsize>,
    done: mpsc::Receiver<()>,
    status: Arc<Mutex<(DaRouteStatus, Option<DaRouteError>)>>,
}
impl RouteService {
    pub fn new() -> std::io::Result<Self> {
        let resolver = ResolverPool::new()?;
        let (jobs, rx) = mpsc::sync_channel(CAPACITY);
        let (done_tx, done) = mpsc::sync_channel(1);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let status = Arc::new(Mutex::new((DaRouteStatus::Idle, None)));
        let worker_status = Arc::clone(&status);
        std::thread::Builder::new()
            .name("da-route-service".into())
            .spawn(move || {
                run(rx, resolver, worker_stop, worker_status);
                let _ = done_tx.try_send(());
            })?;
        Ok(Self {
            jobs,
            stop,
            permits: Arc::new(AtomicUsize::new(0)),
            done,
            status,
        })
    }
    pub fn status(&self) -> (DaRouteStatus, Option<DaRouteError>) {
        self.status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
    /// Nonblocking, bounded enqueue. The ticket controls cancellation and
    /// keeps the lease alive after successful completion.
    pub fn prepare(
        &self,
        host: String,
        adapter: Option<AdapterInfo>,
        local: Vec<IpAddr>,
        deadline: Instant,
    ) -> Result<RouteTicket, DaRouteError> {
        self.permits
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < CAPACITY).then_some(count + 1)
            })
            .map_err(|_| DaRouteError::Resolve("route service busy".into()))?;
        let state = Arc::new(State {
            cancelled: AtomicBool::new(false),
            result: Mutex::new(None),
            permit_live: AtomicBool::new(true),
            permits: Arc::clone(&self.permits),
        });
        let ticket = RouteTicket {
            state: Arc::clone(&state),
        };
        self.jobs
            .try_send(Job {
                host,
                adapter,
                local,
                deadline,
                state,
            })
            .map_err(|_| DaRouteError::Resolve("route service busy or stopped".into()))?;
        Ok(ticket)
    }
}
impl Drop for RouteService {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        // A helper call is bounded; do not wait indefinitely for a resolver or
        // platform operation. On timeout the service thread is detached.
        let _ = self.done.recv_timeout(Duration::from_secs(10));
    }
}
fn run(
    rx: mpsc::Receiver<Job>,
    resolver: ResolverPool,
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<(DaRouteStatus, Option<DaRouteError>)>>,
) {
    let mut manager = DaRouteManager::new();
    let mut pending: Vec<Pending> = Vec::new();
    let mut held: Vec<Held> = Vec::new();
    let mut refresh = Instant::now();
    while !stop.load(Ordering::Acquire) {
        match rx.recv_timeout(Duration::from_millis(20)) {
            Ok(job) => {
                if !job.state.cancelled.load(Ordering::Acquire) {
                    match resolver.submit(job.host.clone(), job.deadline) {
                        Ok(dns) => pending.push(Pending { job, dns }),
                        Err(error) => job.state.finish(Err(error)),
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        let mut index = 0;
        while index < pending.len() {
            let ready = pending[index].dns.poll();
            if pending[index].job.state.cancelled.load(Ordering::Acquire) {
                pending.swap_remove(index);
                continue;
            }
            let Some(result) = ready else {
                index += 1;
                continue;
            };
            let task = pending.swap_remove(index);
            let state = &task.job.state;
            let result = result.and_then(|targets| {
                manager.prepare_resolved(
                    targets,
                    task.job.adapter.as_ref(),
                    &task.job.local,
                    || {
                        stop.load(Ordering::Acquire)
                            || state.cancelled.load(Ordering::Acquire)
                            || Instant::now() >= task.job.deadline
                    },
                )
            });
            match result {
                Ok(lease)
                    if !state.cancelled.load(Ordering::Acquire)
                        && !stop.load(Ordering::Acquire)
                        && Instant::now() < task.job.deadline =>
                {
                    state.finish(Ok(()));
                    held.push(Held {
                        state: Arc::clone(state),
                        lease,
                    });
                }
                Ok(lease) => {
                    manager.release(lease);
                    if !state.cancelled.load(Ordering::Acquire) {
                        state.finish(Err(DaRouteError::Resolve(
                            "route preparation timed out or stopped".into(),
                        )));
                    }
                }
                Err(error) => state.finish(Err(error)),
            }
        }
        let mut index = 0;
        while index < held.len() {
            if stop.load(Ordering::Acquire) {
                break;
            }
            if held[index].state.cancelled.load(Ordering::Acquire) {
                manager.release(held.swap_remove(index).lease);
            } else if !manager.lease_is_valid(&held[index].lease) {
                let item = held.swap_remove(index);
                item.state.finish(Err(DaRouteError::HelperUnavailable(
                    "route lease lost".into(),
                )));
                manager.release(item.lease);
            } else {
                index += 1;
            }
        }
        *status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            (manager.status(), manager.last_error().cloned());
        if refresh.elapsed() >= Duration::from_secs(1800) {
            manager.refresh();
            refresh = Instant::now();
        }
    }
    let stopped = || DaRouteError::HelperUnavailable("route service stopped".into());
    for item in pending {
        item.job.state.finish(Err(stopped()));
    }
    for item in held {
        item.state.finish(Err(stopped()));
    }
    for job in rx.try_iter() {
        job.state.finish(Err(stopped()));
    }
    // Manager drop closes the helper pipe, releasing all helper-owned routes.
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dropping_ticket_is_reaped_without_a_release_queue() {
        let service = RouteService::new().unwrap();
        let ticket = service
            .prepare(
                "127.0.0.1".into(),
                None,
                vec![],
                Instant::now() + Duration::from_secs(2),
            )
            .unwrap();
        let wait = Instant::now() + Duration::from_secs(2);
        while ticket.result().is_none() && Instant::now() < wait {
            std::thread::yield_now();
        }
        assert!(matches!(ticket.result(), Some(Ok(()))));
        let state = Arc::clone(&ticket.state);
        drop(ticket);
        let wait = Instant::now() + Duration::from_secs(2);
        while Arc::strong_count(&state) != 1 && Instant::now() < wait {
            std::thread::yield_now();
        }
        assert_eq!(Arc::strong_count(&state), 1);
    }
    #[test]
    fn completed_tickets_do_not_limit_retained_sessions() {
        let service = RouteService::new().unwrap();
        let mut tickets = Vec::new();
        for _ in 0..CAPACITY * 2 {
            let deadline = Instant::now() + Duration::from_secs(2);
            let ticket = service
                .prepare("127.0.0.1".into(), None, vec![], deadline)
                .unwrap();
            while ticket.result().is_none() && Instant::now() < deadline {
                std::thread::yield_now();
            }
            assert!(matches!(ticket.result(), Some(Ok(()))));
            tickets.push(ticket);
        }
        let wait = Instant::now() + Duration::from_secs(2);
        while service.permits.load(Ordering::Acquire) != 0 && Instant::now() < wait {
            std::thread::yield_now();
        }
        assert_eq!(service.permits.load(Ordering::Acquire), 0);
        drop(tickets);
        assert_eq!(service.permits.load(Ordering::Acquire), 0);
    }
}
