//! One lazily initialized runtime with a continuously running reactor/heartbeat worker.

use crate::{Error, Result};
use std::{
    future::Future,
    sync::{Mutex, OnceLock},
};
use tokio::runtime::{Builder, Runtime};

/// Process-lifetime ownership prevents runtime destruction from an async Drop context.
static RUNTIME: OnceLock<Runtime> = OnceLock::new();
/// Serialize initialization without starting redundant worker threads under contention.
static INITIALIZING: Mutex<()> = Mutex::new(());

/// Reject nested runtime entry before touching the network or initializing resources.
pub(super) fn check_context() -> Result<()> {
    if tokio::runtime::Handle::try_current().is_ok() {
        return Err(Error::BlockingInAsyncContext);
    }
    Ok(())
}
/// Get the shared runtime, preserving a fallible OS thread/driver initialization.
fn runtime() -> Result<&'static Runtime> {
    if let Some(runtime) = RUNTIME.get() {
        return Ok(runtime);
    }
    let _guard = INITIALIZING
        .lock()
        .map_err(|_| Error::Protocol("blocking runtime initialization lock poisoned"))?;
    if let Some(runtime) = RUNTIME.get() {
        return Ok(runtime);
    }
    let runtime = Builder::new_multi_thread()
        .worker_threads(1)
        .thread_name("rs-snap7-io")
        .enable_all()
        .build()?;
    RUNTIME
        .set(runtime)
        .map_err(|_| Error::Protocol("blocking runtime initialized twice"))?;
    RUNTIME
        .get()
        .ok_or(Error::Protocol("blocking runtime initialization failed"))
}
/// Borrow a core future directly; neither arguments nor results need Send or 'static.
/// Callbacks are drained after exiting the runtime so synchronous hook reentry is safe.
pub(crate) fn run<T>(future: impl Future<Output = Result<T>>) -> Result<T> {
    check_context()?;
    let capture = super::hooks::Capture::new();
    let result = runtime()?.block_on(super::hooks::capture(capture.clone(), future));
    super::hooks::finish(capture);
    result
}

#[cfg(test)]
mod tests {
    //! Runtime ownership remains process-wide under simultaneous callers.
    use super::*;
    /// Racing initialization and repeated use always return the same runtime.
    #[test]
    fn concurrent_callers_share_one_process_runtime() {
        let workers: Vec<_> = (0..8)
            .map(|_| {
                std::thread::spawn(|| {
                    let address = runtime().unwrap() as *const Runtime as usize;
                    assert_eq!(run(async { Ok(42) }).unwrap(), 42);
                    address
                })
            })
            .collect();
        let addresses: Vec<_> = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect();
        assert!(addresses.iter().all(|address| *address == addresses[0]));
    }
    /// Entered blocking-pool threads use the same explicit context rejection rule.
    #[tokio::test]
    async fn entered_spawn_blocking_context_is_rejected_without_panic() {
        let result = tokio::task::spawn_blocking(|| run(async { Ok(()) }))
            .await
            .unwrap();
        assert!(matches!(result, Err(Error::BlockingInAsyncContext)));
    }
}
