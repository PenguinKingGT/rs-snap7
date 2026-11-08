//! Opt-in Tokio heartbeat with weak session ownership and cancellation-safe shutdown.

use super::Client;
use std::sync::Arc;

/// Aborts background work when the last client clone releases its handle.
#[derive(Debug)]
pub(super) struct HeartbeatHandle(tokio::task::AbortHandle);

impl Drop for HeartbeatHandle {
    /// Cancellation drops any in-flight socket owned by the background exchange.
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl Client {
    /// Schedule one guarded probe per interval, skipping manually closed sessions.
    pub(super) fn start_heartbeat(&self) -> Option<Arc<HeartbeatHandle>> {
        let interval = self.config.recovery.heartbeat_interval?;
        let session = Arc::downgrade(&self.inner);
        let hook = self.config.connection_hook.clone();
        let task = tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                let Some(inner) = session.upgrade() else {
                    break;
                };
                let mut guard = inner.lock().await;
                if guard.manually_closed() {
                    continue;
                }
                guard.start_operation();
                let _ = guard.heartbeat().await;
                let events = guard.take_events();
                drop(guard);
                hook.emit(events);
            }
        });
        Some(Arc::new(HeartbeatHandle(task.abort_handle())))
    }
}
