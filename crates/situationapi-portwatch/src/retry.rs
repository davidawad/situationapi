use rand::Rng;
use std::time::Duration;

/// Retry settings for transient transport, throttling, and server errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Total attempts, including the first. One disables retries.
    pub max_attempts: u32,
    /// Backoff ceiling for the first retry; it doubles on subsequent retries.
    pub base_delay: Duration,
    /// Upper bound for any wait, including a server `Retry-After` hint.
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 4,
            base_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(20),
        }
    }
}

impl RetryPolicy {
    pub(crate) fn delay(self, retry: u32, retry_after: Option<Duration>) -> Duration {
        let ceiling = self
            .base_delay
            .saturating_mul(2u32.saturating_pow(retry.min(16)))
            .min(self.max_delay);
        let jitter = rand::rng().random_range(0.0..=1.0);
        let backoff = ceiling.mul_f64(jitter);
        retry_after
            .map_or(backoff, |hint| hint.max(backoff))
            .min(self.max_delay)
    }

    pub(crate) fn retryable_status(status: u16) -> bool {
        status == 408 || status == 429 || (500..=599).contains(&status)
    }
}

#[cfg(test)]
mod tests {
    use super::RetryPolicy;

    #[test]
    fn retries_only_throttling_timeout_and_server_statuses() {
        assert!(
            RetryPolicy::retryable_status(408),
            "request timeout retries"
        );
        assert!(RetryPolicy::retryable_status(429), "throttling retries");
        assert!(RetryPolicy::retryable_status(503), "server errors retry");
        assert!(
            !RetryPolicy::retryable_status(404),
            "client errors do not retry"
        );
        assert!(
            !RetryPolicy::retryable_status(200),
            "success is not retryable"
        );
    }
}
