//! Bounded retry policy for rate-limited requests.
//!
//! Only `429` responses are retried: a 429 means the request was not processed,
//! so replaying it cannot duplicate a write even for a non-idempotent command.
//! The loop is always bounded by [`RetryPolicy::max_retries`], and a
//! `Retry-After` longer than [`RetryPolicy::max_retry_after`] is surfaced
//! instead of slept through.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::error::{Error, Result};

/// Default retry policy: two retries, exponential backoff, 429 only.
pub const DEFAULT_RETRY: RetryPolicy = RetryPolicy {
    max_retries: 2,
    base_delay: Duration::from_millis(250),
    max_delay: Duration::from_millis(4_000),
    jitter: false,
    max_retry_after: Duration::from_millis(60_000),
};

/// One retry budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Retries after the first attempt. Never unbounded.
    pub max_retries: u32,
    /// First exponential delay when the server sent no `Retry-After`.
    pub base_delay: Duration,
    /// Ceiling for exponential delays.
    pub max_delay: Duration,
    /// Add up to 50% jitter to exponential delays.
    pub jitter: bool,
    /// A `Retry-After` longer than this is surfaced, not slept through.
    pub max_retry_after: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        DEFAULT_RETRY
    }
}

impl RetryPolicy {
    /// A policy that never retries.
    pub fn none() -> Self {
        Self {
            max_retries: 0,
            ..Self::default()
        }
    }

    /// The delay before retry number `attempt` (0-based), honouring
    /// `Retry-After` when present.
    ///
    /// Returns [`Error::RetryAfterTooLong`] when the server asks for longer
    /// than [`Self::max_retry_after`].
    pub fn delay(&self, attempt: u32, retry_after_ms: Option<u64>) -> Result<Duration> {
        if let Some(requested) = retry_after_ms {
            let limit = duration_ms(self.max_retry_after);
            if requested > limit {
                return Err(Error::RetryAfterTooLong {
                    requested_ms: requested,
                    limit_ms: limit,
                });
            }
            return Ok(Duration::from_millis(requested));
        }
        let exponent = attempt.min(31);
        let scaled = self
            .base_delay
            .checked_mul(1u32 << exponent)
            .unwrap_or(self.max_delay);
        let base = scaled.min(self.max_delay);
        if !self.jitter {
            return Ok(base);
        }
        // Deterministic-free jitter without a random dependency: use the clock's
        // sub-second component as an entropy source and keep the factor in
        // 0.5..=1.0 so the delay never exceeds the cap.
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.subsec_nanos())
            .unwrap_or(0);
        let factor = 0.5 + f64::from(nanos % 1_000) / 2_000.0;
        let millis = (base.as_secs_f64() * 1_000.0 * factor) as u64;
        Ok(Duration::from_millis(
            millis.min(duration_ms(self.max_delay)),
        ))
    }
}

/// A duration as whole milliseconds, saturating at `u64::MAX`.
pub(crate) fn duration_ms(duration: Duration) -> u64 {
    duration.as_millis().try_into().unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_bounded() {
        assert_eq!(DEFAULT_RETRY.max_retries, 2);
        assert_eq!(
            DEFAULT_RETRY.delay(0, None).expect("delay"),
            Duration::from_millis(250)
        );
        assert_eq!(
            DEFAULT_RETRY.delay(1, None).expect("delay"),
            Duration::from_millis(500)
        );
        assert_eq!(
            DEFAULT_RETRY.delay(9, None).expect("delay"),
            Duration::from_millis(4_000)
        );
    }

    #[test]
    fn retry_after_replaces_backoff() {
        assert_eq!(
            DEFAULT_RETRY.delay(0, Some(120)).expect("delay"),
            Duration::from_millis(120)
        );
    }

    #[test]
    fn an_excessive_retry_after_is_an_error() {
        let error = DEFAULT_RETRY.delay(0, Some(60_001)).expect_err("must fail");
        assert!(matches!(error, Error::RetryAfterTooLong { .. }));
    }

    #[test]
    fn jitter_stays_within_the_cap() {
        let policy = RetryPolicy {
            jitter: true,
            ..DEFAULT_RETRY
        };
        let delay = policy.delay(2, None).expect("delay");
        assert!(delay >= Duration::from_millis(500));
        assert!(delay <= Duration::from_millis(4_000));
    }

    #[test]
    fn none_disables_retries_but_keeps_the_policy() {
        assert_eq!(RetryPolicy::none().max_retries, 0);
        assert_eq!(RetryPolicy::none().max_delay, DEFAULT_RETRY.max_delay);
    }
}
