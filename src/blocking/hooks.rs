//! Deferred blocking hooks with scoped caller delivery and bounded background dispatch.

use crate::{Error, Result};
use std::{
    cell::Cell,
    future::Future,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, SyncSender},
    },
};

/// Accepted events are FIFO; a slow sink cannot create an unbounded background queue.
const CAPACITY: usize = 1024;
/// Process-lifetime sender owns one callback dispatcher, created only for enabled hooks.
static DISPATCHER: OnceLock<SyncSender<Job>> = OnceLock::new();
/// Serialize the only dispatcher initialization.
static INITIALIZING: Mutex<()> = Mutex::new(());
thread_local! {
    /// Reentrant operations from a background hook enqueue their completion hooks.
    static DISPATCHING:Cell<bool>=const {Cell::new(false)};
}
tokio::task_local! {
    /// Explicitly inherited by pool tasks, never automatically inherited by heartbeats.
    static CAPTURE:Arc<Capture>;
}

/// Per-client drop accounting; no payloads or passwords enter callback jobs.
#[derive(Debug)]
pub(crate) struct Delivery {
    sender: SyncSender<Job>,
    dropped: AtomicU64,
}
/// Application invocation and its queue-overflow accounting owner.
struct Job {
    delivery: Arc<Delivery>,
    call: Box<dyn FnOnce() + Send>,
}
impl Job {
    /// Last-resort isolation keeps the shared callback dispatcher alive.
    fn invoke(self) {
        let _ = catch_unwind(AssertUnwindSafe(self.call));
    }
    /// Count a rejected event on the client that produced it.
    fn enqueue(self) {
        let sender = self.delivery.sender.clone();
        if let Err(error) = sender.try_send(self) {
            let job = match error {
                mpsc::TrySendError::Full(job) | mpsc::TrySendError::Disconnected(job) => job,
            };
            job.delivery.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}
impl Delivery {
    /// Initialize one bounded process-wide dispatcher when application hooks are configured.
    pub(crate) fn new() -> Result<Arc<Self>> {
        if DISPATCHER.get().is_none() {
            let _guard = INITIALIZING
                .lock()
                .map_err(|_| Error::Protocol("callback initialization lock poisoned"))?;
            if DISPATCHER.get().is_none() {
                let (sender, receiver) = mpsc::sync_channel::<Job>(CAPACITY);
                std::thread::Builder::new()
                    .name("rs-snap7-hooks".into())
                    .spawn(move || {
                        DISPATCHING.with(|flag| flag.set(true));
                        for job in receiver {
                            job.invoke();
                        }
                    })?;
                DISPATCHER
                    .set(sender)
                    .map_err(|_| Error::Protocol("callback dispatcher initialized twice"))?;
            }
        }
        Ok(Arc::new(Self {
            sender: DISPATCHER
                .get()
                .ok_or(Error::Protocol("callback dispatcher missing"))?
                .clone(),
            dropped: AtomicU64::new(0),
        }))
    }
    /// Snapshot dropped background notifications without acquiring a session guard.
    pub(crate) fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
    /// Capture an in-call hook or enqueue background/late events without blocking I/O.
    pub(crate) fn dispatch(self: &Arc<Self>, call: impl FnOnce() + Send + 'static) {
        let job = Job {
            delivery: self.clone(),
            call: Box::new(call),
        };
        let capture = CAPTURE.try_with(Arc::clone).ok();
        if let Some(capture) = capture {
            let mut state = capture.0.lock().unwrap_or_else(|p| p.into_inner());
            if state.open {
                state.jobs.push(job);
                return;
            }
        }
        job.enqueue();
    }
}
/// Capture closes before callbacks execute; aborted pool tasks cannot append to a dead caller.
struct State {
    open: bool,
    jobs: Vec<Job>,
}
/// One blocking logical invocation's deferred notifications.
pub(crate) struct Capture(Mutex<State>);
impl Capture {
    /// Create an open capture whose lifetime can be shared with spawned pool work.
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self(Mutex::new(State {
            open: true,
            jobs: Vec::new(),
        })))
    }
}
/// Attach hook delivery to future polls rather than to an executor thread.
pub(crate) async fn capture<T>(scope: Arc<Capture>, future: impl Future<Output = T>) -> T {
    CAPTURE.scope(scope, future).await
}
/// Propagate the current caller scope through explicitly spawned operation tasks.
pub(crate) fn inherit<T>(future: impl Future<Output = T>) -> impl Future<Output = T> {
    let scope = CAPTURE.try_with(Arc::clone).ok();
    async move {
        match scope {
            Some(scope) => capture(scope, future).await,
            None => future.await,
        }
    }
}
/// Close capture and invoke hooks after block_on returned, without any capture lock held.
pub(super) fn finish(capture: Arc<Capture>) {
    let jobs = {
        let mut state = capture.0.lock().unwrap_or_else(|p| p.into_inner());
        state.open = false;
        std::mem::take(&mut state.jobs)
    };
    for job in jobs {
        if DISPATCHING.with(Cell::get) {
            job.enqueue();
        } else {
            job.invoke();
        }
    }
}

#[cfg(test)]
mod tests {
    //! Deterministic queue/drop and late-delivery checks without global dispatcher contention.
    use super::*;

    /// A full queue drops the incoming event and charges only its originating client.
    #[test]
    fn full_background_queue_counts_dropped_events_without_waiting() {
        let (sender, receiver) = mpsc::sync_channel(1);
        let delivery = Arc::new(Delivery {
            sender,
            dropped: AtomicU64::new(0),
        });
        let invoked = Arc::new(AtomicU64::new(0));
        let count = invoked.clone();
        delivery.dispatch(move || {
            count.fetch_add(1, Ordering::Relaxed);
        });
        delivery.dispatch(|| panic!("overflowed callback must not run"));
        assert_eq!(delivery.dropped(), 1);
        receiver.try_recv().unwrap().invoke();
        assert_eq!(invoked.load(Ordering::Relaxed), 1);
        assert!(receiver.try_recv().is_err());
    }

    /// Closing a caller capture routes late pool completions to background delivery.
    #[test]
    fn closed_capture_falls_back_and_caller_hooks_run_after_runtime_exit() {
        let (sender, receiver) = mpsc::sync_channel(2);
        let delivery = Arc::new(Delivery {
            sender,
            dropped: AtomicU64::new(0),
        });
        let caller = std::thread::current().id();
        let scope = Capture::new();
        let d = delivery.clone();
        let scoped = scope.clone();
        super::super::runtime::run(capture(scoped, async move {
            d.dispatch(move || {
                assert_eq!(std::thread::current().id(), caller);
                assert!(tokio::runtime::Handle::try_current().is_err());
            });
            Ok(())
        }))
        .unwrap();
        assert!(receiver.try_recv().is_err());
        finish(scope.clone());
        super::super::runtime::run(capture(scope, async move {
            delivery.dispatch(|| {});
            Ok(())
        }))
        .unwrap();
        receiver.try_recv().unwrap().invoke();
    }
}
