//! Logical operation events with redacted context and application-owned sinks.

use crate::Error;
use std::{
    fmt,
    net::SocketAddr,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

/// Completed logical operation, with no request payloads or credentials.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationEvent {
    /// Public operation name, emitted once even when convenience methods nest.
    pub operation: &'static str,
    /// PLC endpoint.
    pub endpoint: SocketAddr,
    /// Total elapsed wall-clock time, including session queueing and waits.
    pub elapsed: Duration,
    /// Whether the operation returned an error.
    pub failed: bool,
    /// Stable error category, absent on success.
    pub error_kind: Option<&'static str>,
    /// Available protocol/item code, including causes of partial writes.
    pub error_code: Option<u16>,
}

impl OperationEvent {
    /// Serialize a single structured JSON log record without including data buffers.
    pub fn to_json(&self) -> String {
        let code = self
            .error_code
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_owned());
        let kind = self
            .error_kind
            .map(|v| format!("\"{}\"", escape(v)))
            .unwrap_or_else(|| "null".to_owned());
        format!(
            "{{\"operation\":\"{}\",\"endpoint\":\"{}\",\"elapsed_ns\":{},\"failed\":{},\"error_kind\":{},\"error_code\":{}}}",
            escape(self.operation),
            self.endpoint,
            self.elapsed.as_nanos(),
            self.failed,
            kind,
            code
        )
    }
}

/// Application callback, shared by clones and redacted in Debug output.
/// Callbacks must be short; asynchronous sinks should enqueue events themselves.
#[derive(Clone, Default)]
pub struct OperationHook {
    callback: Option<Arc<dyn Fn(OperationEvent) + Send + Sync>>,
    #[cfg(feature = "blocking")]
    delivery: Option<Arc<crate::blocking::hooks::Delivery>>,
}

impl OperationHook {
    /// Install a callback. It runs after session guards have been released.
    pub fn new(callback: impl Fn(OperationEvent) + Send + Sync + 'static) -> Self {
        Self {
            callback: Some(Arc::new(callback)),
            #[cfg(feature = "blocking")]
            delivery: None,
        }
    }
    /// Whether a sink needs deferred blocking delivery.
    #[cfg(feature = "blocking")]
    pub(crate) fn enabled(&self) -> bool {
        self.callback.is_some()
    }
    /// Preserve the application sink while adapting its execution context.
    #[cfg(feature = "blocking")]
    pub(crate) fn deferred(mut self, delivery: Arc<crate::blocking::hooks::Delivery>) -> Self {
        self.delivery = Some(delivery);
        self
    }
}

impl fmt::Debug for OperationHook {
    /// Never print callback captures, which may contain application secrets.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OperationHook")
            .field("enabled", &self.callback.is_some())
            .finish()
    }
}

/// Snapshot of completed operations for one shared client session.
#[derive(Debug, Clone, Default)]
pub struct OperationStats {
    /// Number of logical operations that returned a result.
    pub completed: u64,
    /// Number of operations returning errors.
    pub failed: u64,
    /// Last completed logical operation.
    pub last: Option<OperationEvent>,
    /// Callback panics isolated from PLC communication.
    pub hook_panics: u64,
    /// Background hook notifications dropped by a full blocking-adapter queue.
    pub background_hook_events_dropped: u64,
}

/// Shared accounting independent of transport locks and runtime selection.
#[derive(Debug)]
pub(crate) struct Observer {
    endpoint: SocketAddr,
    hook: OperationHook,
    stats: Mutex<OperationStats>,
    hook_panics: Arc<AtomicU64>,
}

tokio::task_local! {
    /// Scope is restored per future poll, preventing leakage across joined futures.
    static ASYNC_ACTIVE: Vec<usize>;
}

impl Observer {
    /// Create independent accounting while retaining the configured application sink.
    pub(crate) fn new(endpoint: SocketAddr, hook: OperationHook) -> Self {
        Self {
            endpoint,
            hook,
            stats: Mutex::new(OperationStats::default()),
            hook_panics: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Native async nesting uses future-local scopes; cancelled futures have no
    /// completion record because they did not return a result to their caller.
    pub(crate) fn run_async<T>(
        &self,
        name: &'static str,
        call: impl std::future::Future<Output = crate::Result<T>>,
    ) -> impl std::future::Future<Output = crate::Result<T>> {
        // Box before constructing the returned future: nested convenience calls
        // must not multiply their concrete future layouts on the caller's stack.
        let call = Box::pin(call);
        async move {
            let id = self as *const Self as usize;
            let mut active = ASYNC_ACTIVE.try_with(Clone::clone).unwrap_or_default();
            if active.contains(&id) {
                return call.await;
            }
            active.push(id);
            ASYNC_ACTIVE
                .scope(active, async {
                    let start = Instant::now();
                    let result = call.await;
                    self.record(name, start.elapsed(), result.as_ref().err());
                    result
                })
                .await
        }
    }

    /// Whether an outer async operation still owns the current event scope.
    pub(crate) fn active_async(&self) -> bool {
        ASYNC_ACTIVE
            .try_with(|v| v.contains(&(self as *const Self as usize)))
            .unwrap_or(false)
    }

    /// Snapshot accounting without taking the PLC session lock.
    pub(crate) fn stats(&self) -> OperationStats {
        let mut stats = self.stats.lock().unwrap_or_else(|p| p.into_inner()).clone();
        stats.hook_panics = self.hook_panics.load(Ordering::Relaxed);
        stats
    }

    /// Update counters before calling application code, with no internal locks held.
    fn record(&self, name: &'static str, elapsed: Duration, error: Option<&Error>) {
        let event = OperationEvent {
            operation: name,
            endpoint: self.endpoint,
            elapsed,
            failed: error.is_some(),
            error_kind: error.map(kind),
            error_code: error.and_then(code),
        };
        {
            let mut stats = self.stats.lock().unwrap_or_else(|p| p.into_inner());
            stats.completed = stats.completed.saturating_add(1);
            stats.failed = stats.failed.saturating_add(u64::from(error.is_some()));
            stats.last = Some(event.clone());
        }
        if let Some(callback) = self.hook.callback.clone() {
            let panics = self.hook_panics.clone();
            let invoke = move || {
                if catch_unwind(AssertUnwindSafe(|| callback(event))).is_err() {
                    panics.fetch_add(1, Ordering::Relaxed);
                }
            };
            #[cfg(feature = "blocking")]
            if let Some(delivery) = &self.hook.delivery {
                delivery.dispatch(invoke);
                return;
            }
            invoke();
        }
    }
}

/// Describe errors without serializing potentially sensitive cause messages.
fn kind(error: &Error) -> &'static str {
    match error {
        Error::BlockingInAsyncContext => "blocking_in_async_context",
        Error::InvalidInput(_) => "invalid_input",
        Error::Protocol(_) | Error::StaleReference { .. } | Error::LostReference { .. } => {
            "protocol"
        }
        Error::Io(_) => "io",
        Error::Timeout | Error::DeadlineBeforeSend => "timeout",
        Error::Disconnected => "disconnected",
        Error::Plc(_) | Error::Item(_) => "plc",
        Error::Authentication(_) => "authentication",
        Error::RateLimited => "rate_limited",
        Error::Unsupported(_) => "unsupported",
        Error::PartialWrite { .. } => "partial_write",
        Error::AmbiguousWrite { .. } => "ambiguous_write",
        Error::AmbiguousOperation { .. } => "ambiguous_operation",
    }
}

/// Retain numeric error codes recursively through known progress wrappers.
fn code(error: &Error) -> Option<u16> {
    match error {
        Error::Plc(code) | Error::Authentication(code) => Some(*code),
        Error::Item(code) => Some(u16::from(*code)),
        Error::PartialWrite { source, .. }
        | Error::AmbiguousWrite { source, .. }
        | Error::AmbiguousOperation { source, .. } => code(source),
        _ => None,
    }
}

/// Escape JSON strings, including control characters, without extra dependencies.
fn escape(text: &str) -> String {
    let mut result = String::new();
    for c in text.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c < ' ' => result.push_str(&format!("\\u{:04x}", c as u32)),
            c => result.push(c),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    //! Redaction, JSON escaping, and callback isolation tests.
    use super::*;

    /// JSON contains escaped text, typed timing, and protocol codes, with no raw errors.
    #[test]
    fn json_record_escapes_strings_and_retains_code() {
        let event = OperationEvent {
            operation: "read\"\n",
            endpoint: "127.0.0.1:102".parse().unwrap(),
            elapsed: Duration::from_nanos(123),
            failed: true,
            error_kind: Some("plc"),
            error_code: Some(5),
        };
        assert_eq!(
            event.to_json(),
            "{\"operation\":\"read\\\"\\n\",\"endpoint\":\"127.0.0.1:102\",\"elapsed_ns\":123,\"failed\":true,\"error_kind\":\"plc\",\"error_code\":5}"
        );
        assert_eq!(escape("\0\\\r\t"), "\\u0000\\\\\\r\\t");
    }

    /// A faulty application sink cannot change a PLC result or poison internal locks.
    #[tokio::test]
    async fn callback_panics_do_not_change_operation_results() {
        let observer = Observer::new(
            "127.0.0.1:102".parse().unwrap(),
            OperationHook::new(|_| panic!("sink failed")),
        );
        assert_eq!(
            observer.run_async("read", async { Ok(42) }).await.unwrap(),
            42
        );
        let result: crate::Result<()> = observer
            .run_async("write", async {
                Err(Error::PartialWrite {
                    confirmed: 1,
                    source: Box::new(Error::Item(5)),
                })
            })
            .await;
        assert!(result.is_err());
        let stats = observer.stats();
        assert_eq!(stats.completed, 2);
        assert_eq!(stats.failed, 1);
        assert_eq!(stats.hook_panics, 2);
        assert_eq!(stats.last.unwrap().error_code, Some(5));
    }
}
