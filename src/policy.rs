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
