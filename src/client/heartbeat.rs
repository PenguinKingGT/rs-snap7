//! Opt-in heartbeat worker with weak session ownership and interruptible shutdown.

use super::Client;
use crate::Result;
use std::sync::{Arc, Condvar, Mutex};

/// Retained only by client clones; workers retain the stop signal, not this handle.
#[derive(Debug)]
pub(super) struct HeartbeatHandle {
    stop: Arc<(Mutex<bool>, Condvar)>,
}

impl Drop for HeartbeatHandle {
    /// Wake an idle worker when the last client clone is dropped.
    fn drop(&mut self) {
        let (flag, changed) = &*self.stop;
        *flag.lock().unwrap_or_else(|p| p.into_inner()) = true;
        changed.notify_all();
    }
}

impl Client {
    /// Start an optional worker which never reads a socket outside the session guard.
    pub(super) fn start_heartbeat(&self) -> Result<Option<Arc<HeartbeatHandle>>> {
        let Some(interval) = self.config.recovery.heartbeat_interval else {
            return Ok(None);
        };
        let stop = Arc::new((Mutex::new(false), Condvar::new()));
        let handle = Arc::new(HeartbeatHandle {
            stop: Arc::clone(&stop),
        });
        let session = Arc::downgrade(&self.inner);
        let hook = self.config.connection_hook.clone();
        std::thread::Builder::new()
            .name("rs-snap7-heartbeat".to_owned())
            .spawn(move || {
                loop {
                    let (flag, changed) = &*stop;
                    let guard = flag.lock().unwrap_or_else(|p| p.into_inner());
                    let (guard, _) = changed
                        .wait_timeout_while(guard, interval, |stopped| !*stopped)
                        .unwrap_or_else(|p| p.into_inner());
                    if *guard {
                        break;
                    }
                    drop(guard);
                    let Some(inner) = session.upgrade() else {
                        break;
                    };
                    let Ok(mut guard) = inner.lock() else {
                        break;
                    };
                    if guard.manually_closed() {
                        continue;
                    }
                    guard.start_operation();
                    let _ = guard.heartbeat();
                    let events = guard.take_events();
                    drop(guard);
                    hook.emit(events);
                }
            })?;
        Ok(Some(handle))
    }
}
