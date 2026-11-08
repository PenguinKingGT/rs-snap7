//! Deterministic per-session request limiting, shared by blocking and async I/O.

use crate::{Error, Result};
use std::time::Duration;

/// Request scheduling algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateAlgorithm {
    /// Evenly space successful request grants.
    Fixed,
    /// Accumulate tokens up to a bounded burst capacity.
    TokenBucket,
}

/// Behavior when a request cannot be granted immediately.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateBehavior {
    /// Wait within the current operation deadline.
    Block,
    /// Return `Error::RateLimited` without sleeping or sending.
    Reject,
}

/// Per-client wire request policy; a zero rate disables limiting.
#[derive(Debug, Clone, Copy)]
pub struct RateLimit {
    /// Permitted requests per second; must be finite and nonnegative.
    pub max_per_second: f64,
    /// Fixed spacing or token bucket.
    pub algorithm: RateAlgorithm,
    /// Wait or reject when no permit is available.
    pub behavior: RateBehavior,
    /// Positive token bucket capacity; fixed spacing ignores this field.
    pub burst: u32,
}

impl Default for RateLimit {
    /// Disable limiting while retaining conservative one-request burst defaults.
    fn default() -> Self {
        Self {
            max_per_second: 0.0,
            algorithm: RateAlgorithm::Fixed,
            behavior: RateBehavior::Block,
            burst: 1,
        }
    }
}

impl RateLimit {
    /// Reject invalid floating-point rates and unrepresentable request intervals.
    pub fn validate(self) -> Result<()> {
        if !self.max_per_second.is_finite() || self.max_per_second < 0.0 || self.burst == 0 {
            return Err(Error::InvalidInput(
                "invalid request rate or burst capacity",
            ));
        }
        if self.max_per_second > 0.0 {
            let interval = Duration::try_from_secs_f64(1.0 / self.max_per_second)
                .map_err(|_| Error::InvalidInput("request interval out of range"))?;
            if interval.is_zero() {
                return Err(Error::InvalidInput(
                    "request interval below clock resolution",
                ));
            }
        }
        Ok(())
    }
}

/// Pure monotonic-time accounting. The owning session provides synchronization
/// and elapsed time; tests can advance time without sleeping.
#[derive(Debug)]
pub(crate) struct Limiter {
    config: RateLimit,
    next: Duration,
    last: Duration,
    tokens: f64,
}

impl Limiter {
    /// Initialize a full token bucket and an immediately available fixed permit.
    pub(crate) fn new(config: RateLimit) -> Result<Self> {
        config.validate()?;
        Ok(Self {
            tokens: f64::from(config.burst),
            config,
            next: Duration::ZERO,
            last: Duration::ZERO,
        })
    }

    /// Return zero and consume a permit, or return the required wait without
    /// reserving a future permit. Cancelled waits therefore consume no token.
    pub(crate) fn poll(&mut self, now: Duration) -> Result<Duration> {
        let rate = self.config.max_per_second;
        if rate == 0.0 {
            return Ok(Duration::ZERO);
        }
        let wait = match self.config.algorithm {
            RateAlgorithm::Fixed => self.next.saturating_sub(now),
            RateAlgorithm::TokenBucket => {
                let elapsed = now.saturating_sub(self.last).as_secs_f64();
                self.tokens = (self.tokens + elapsed * rate).min(f64::from(self.config.burst));
                self.last = now;
                if self.tokens >= 1.0 {
                    Duration::ZERO
                } else {
                    Duration::try_from_secs_f64((1.0 - self.tokens) / rate)
                        .map_err(|_| Error::InvalidInput("request interval out of range"))?
                        .max(Duration::from_nanos(1))
                }
            }
        };
        if !wait.is_zero() {
            return if self.config.behavior == RateBehavior::Reject {
                Err(Error::RateLimited)
            } else {
                Ok(wait)
            };
        }
        match self.config.algorithm {
            RateAlgorithm::Fixed => {
                let interval = Duration::try_from_secs_f64(1.0 / rate)
                    .map_err(|_| Error::InvalidInput("request interval out of range"))?;
                self.next = now
                    .checked_add(interval)
                    .ok_or(Error::InvalidInput("limiter time overflow"))?;
            }
            RateAlgorithm::TokenBucket => self.tokens -= 1.0,
        }
        Ok(Duration::ZERO)
    }
}

#[cfg(test)]
mod tests {
    //! Fake-time policy tests have no wall-clock sleeps.
    use super::*;

    /// Fixed requests are evenly spaced; polling a future slot does not reserve it.
    #[test]
    fn fixed_spacing_and_cancelled_waits() {
        let mut limiter = Limiter::new(RateLimit {
            max_per_second: 2.0,
            ..Default::default()
        })
        .unwrap();
        assert_eq!(limiter.poll(Duration::ZERO).unwrap(), Duration::ZERO);
        assert_eq!(
            limiter.poll(Duration::from_millis(100)).unwrap(),
            Duration::from_millis(400)
        );
        assert_eq!(
            limiter.poll(Duration::from_millis(500)).unwrap(),
            Duration::ZERO
        );
        assert_eq!(
            limiter.poll(Duration::from_millis(700)).unwrap(),
            Duration::from_millis(300)
        );
    }

    /// Burst capacity bounds credit after long idle periods and preserves fractional refill.
    #[test]
    fn token_bucket_burst_and_fractional_refill() {
        let config = RateLimit {
            max_per_second: 2.0,
            burst: 2,
            algorithm: RateAlgorithm::TokenBucket,
            behavior: RateBehavior::Block,
        };
        let mut limiter = Limiter::new(config).unwrap();
        assert!(limiter.poll(Duration::ZERO).unwrap().is_zero());
        assert!(limiter.poll(Duration::ZERO).unwrap().is_zero());
        assert_eq!(
            limiter.poll(Duration::from_millis(250)).unwrap(),
            Duration::from_millis(250)
        );
        assert!(limiter.poll(Duration::from_millis(500)).unwrap().is_zero());
        for _ in 0..2 {
            assert!(limiter.poll(Duration::from_secs(100)).unwrap().is_zero());
        }
        assert!(!limiter.poll(Duration::from_secs(100)).unwrap().is_zero());
    }

    /// Disabled policies grant immediately; rejection never consumes future credit.
    #[test]
    fn disabled_rejection_and_invalid_configuration() {
        let mut disabled = Limiter::new(RateLimit::default()).unwrap();
        for _ in 0..100 {
            assert!(disabled.poll(Duration::ZERO).unwrap().is_zero());
        }
        let mut rejecting = Limiter::new(RateLimit {
            max_per_second: 1.0,
            behavior: RateBehavior::Reject,
            ..Default::default()
        })
        .unwrap();
        rejecting.poll(Duration::ZERO).unwrap();
        assert!(matches!(
            rejecting.poll(Duration::ZERO),
            Err(Error::RateLimited)
        ));
        assert!(rejecting.poll(Duration::from_secs(1)).unwrap().is_zero());
        for rate in [f64::NAN, f64::INFINITY, -1.0, 1e300, 1e-300] {
            assert!(
                RateLimit {
                    max_per_second: rate,
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
        }
    }
}

/// Automatic recovery settings. Read-only requests may be replayed once after
/// recovery; mutating requests with uncertain outcomes are never replayed.
#[derive(Debug, Clone)]
pub struct Recovery {
    /// Enable bounded reconnect attempts after connection loss.
    pub enabled: bool,
    /// Maximum connection attempts per recovery episode.
    pub max_retries: u32,
    /// Delay before the second attempt; the first attempt is immediate.
    pub initial_delay: Duration,
    /// Multiplier applied to each successive delay, at least one.
    pub backoff_factor: f64,
    /// Upper bound on each retry delay.
    pub max_delay: Duration,
    /// Symmetric jitter fraction, between zero and one.
    pub jitter: f64,
    /// Optional background SZL liveness probe interval; zero is invalid.
    pub heartbeat_interval: Option<Duration>,
}

impl Default for Recovery {
    /// Disable recovery/heartbeat unless explicitly requested.
    fn default() -> Self {
        Self {
            enabled: false,
            max_retries: 3,
            initial_delay: Duration::from_secs(1),
            backoff_factor: 2.0,
            max_delay: Duration::from_secs(30),
            jitter: 0.2,
            heartbeat_interval: None,
        }
    }
}

impl Recovery {
    /// Validate retry bounds and all time arithmetic before starting background work.
    pub fn validate(&self) -> Result<()> {
        if self.max_retries == 0
            || self.max_retries > 1000
            || !self.backoff_factor.is_finite()
            || self.backoff_factor < 1.0
            || !self.jitter.is_finite()
            || !(0.0..=1.0).contains(&self.jitter)
            || self.initial_delay > self.max_delay
            || self.heartbeat_interval.is_some_and(|d| d.is_zero())
        {
            return Err(Error::InvalidInput("invalid recovery policy"));
        }
        for delay in [self.max_delay, self.heartbeat_interval.unwrap_or_default()] {
            if std::time::Instant::now().checked_add(delay).is_none() {
                return Err(Error::InvalidInput("recovery duration out of range"));
            }
        }
        Ok(())
    }

    /// Calculate a capped backoff from a supplied deterministic unit sample.
    pub(crate) fn delay(&self, attempt: u32, sample: f64) -> Duration {
        if attempt == 0 {
            return Duration::ZERO;
        }
        let cap = self.max_delay.as_secs_f64();
        let base = (self.initial_delay.as_secs_f64()
            * self.backoff_factor.powf(f64::from(attempt - 1)))
        .min(cap);
        let seconds = (base * (1.0 + self.jitter * (2.0 * sample - 1.0)))
            .min(cap)
            .max(0.0);
        Duration::try_from_secs_f64(seconds).unwrap_or(self.max_delay)
    }
}

/// An unplanned connection transition, dispatched after releasing the session lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionEvent {
    /// A previously ready connection was lost or invalidated.
    Disconnected,
    /// A replacement connection completed fresh COTP/S7 negotiation.
    Reconnected,
}

/// Application transition hook; callback captures are redacted in Debug output.
#[derive(Clone, Default)]
pub struct ConnectionHook {
    callback: Option<std::sync::Arc<dyn Fn(ConnectionEvent) + Send + Sync>>,
    #[cfg(feature = "blocking")]
    delivery: Option<std::sync::Arc<crate::blocking::hooks::Delivery>>,
}

impl ConnectionHook {
    /// Install a short callback, invoked outside session locks.
    pub fn new(callback: impl Fn(ConnectionEvent) + Send + Sync + 'static) -> Self {
        Self {
            callback: Some(std::sync::Arc::new(callback)),
            #[cfg(feature = "blocking")]
            delivery: None,
        }
    }
    /// Whether an application sink requires a blocking delivery adapter.
    #[cfg(feature = "blocking")]
    pub(crate) fn enabled(&self) -> bool {
        self.callback.is_some()
    }
    /// Route events through caller scopes or the bounded background dispatcher.
    #[cfg(feature = "blocking")]
    pub(crate) fn deferred(
        mut self,
        delivery: std::sync::Arc<crate::blocking::hooks::Delivery>,
    ) -> Self {
        self.delivery = Some(delivery);
        self
    }
    /// Dispatch queued transitions, isolating callback panics from PLC communication.
    pub(crate) fn emit(&self, events: Vec<ConnectionEvent>) {
        if let Some(callback) = &self.callback {
            for event in events {
                let callback = callback.clone();
                let invoke = move || {
                    let _ =
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(event)));
                };
                #[cfg(feature = "blocking")]
                if let Some(delivery) = &self.delivery {
                    delivery.dispatch(invoke);
                    continue;
                }
                invoke();
            }
        }
    }
}

impl std::fmt::Debug for ConnectionHook {
    /// Hide callback captures, which may contain application secrets.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectionHook")
            .field("enabled", &self.callback.is_some())
            .finish()
    }
}

/// Recognize failures eligible for reconnect, excluding PLC/protocol/limiter errors.
pub(crate) fn recoverable(error: &Error) -> bool {
    matches!(error, Error::Io(_) | Error::Timeout | Error::Disconnected)
}

/// Only independent, read-only initial jobs can be replayed after losing a reply.
pub(crate) fn replay_safe(job: &crate::protocol::Pdu) -> bool {
    use crate::protocol::PduType;
    match job.kind {
        PduType::Job => job.parameters.first() == Some(&4),
        PduType::UserData if job.parameters.len() == 8 => matches!(
            (job.parameters[5] & 15, job.parameters[6]),
            (3, 1..=3) | (4, 1) | (7, 1)
        ),
        _ => false,
    }
}

/// Independent jitter samples per retry; no cryptographic security is required.
pub(crate) fn jitter_sample() -> f64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static STATE: AtomicU64 = AtomicU64::new(0x243f6a8885a308d3);
    let mut value = STATE.fetch_add(0x9e3779b97f4a7c15, Ordering::Relaxed);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58476d1ce4e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d049bb133111eb);
    ((value ^ (value >> 31)) >> 11) as f64 / (1u64 << 53) as f64
}

#[cfg(test)]
mod recovery_tests {
    //! Deterministic retry timing and replay classification tests.
    use super::*;

    /// Backoff and jitter remain capped, and mutation/continuation jobs cannot replay.
    #[test]
    fn capped_backoff_validation_and_replay_rules() {
        let policy = Recovery::default();
        assert_eq!(policy.delay(0, 0.5), Duration::ZERO);
        assert_eq!(policy.delay(1, 0.5), Duration::from_secs(1));
        assert_eq!(policy.delay(2, 0.5), Duration::from_secs(2));
        assert_eq!(policy.delay(100, 1.0), policy.max_delay);
        assert!(policy.delay(1, 0.0) < policy.delay(1, 1.0));
        assert!(
            Recovery {
                jitter: f64::NAN,
                ..policy.clone()
            }
            .validate()
            .is_err()
        );
        assert!(
            Recovery {
                max_retries: 0,
                ..policy
            }
            .validate()
            .is_err()
        );
        assert!(replay_safe(
            &crate::protocol::read_request(1, &[crate::Address::db(1, 0, 1).unwrap()]).unwrap()
        ));
        assert!(!replay_safe(
            &crate::protocol::write_request(1, &[(crate::Address::db(1, 0, 1).unwrap(), &[0])])
                .unwrap()
        ));
        assert!(!replay_safe(&crate::protocol::userdata_followup(
            1, 4, 1, 2
        )));
    }
}

/// Classify control/security/clock mutations whose lost ACK must be explicit.
/// Memory writes retain their richer byte-progress errors in the operation executor.
pub(crate) fn mutation_failure(job: &crate::protocol::Pdu, error: Error) -> Error {
    use crate::protocol::PduType;
    let operation = match job.kind {
        PduType::Job if matches!(job.parameters.first(), Some(0x28 | 0x29)) => Some("plc_control"),
        PduType::UserData if job.parameters.len() == 8 => {
            match (job.parameters[5] & 15, job.parameters[6]) {
                (5, 1 | 2) => Some("session_password"),
                (7, 2) => Some("set_clock"),
                _ => None,
            }
        }
        _ => None,
    };
    if let Some(operation) = operation
        && matches!(
            error,
            Error::Io(_)
                | Error::Timeout
                | Error::Protocol(_)
                | Error::LostReference { .. }
                | Error::StaleReference { .. }
        )
    {
        return Error::AmbiguousOperation {
            operation,
            source: Box::new(error),
        };
    }
    error
}
