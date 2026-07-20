use std::{future::Future, time::Duration};

use tokio::time::sleep;
use tracing::warn;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_attempts: usize,
    pub initial_delay: Duration,
    pub max_delay: Duration,
}

impl RetryPolicy {
    pub const fn new(max_attempts: usize, initial_delay: Duration, max_delay: Duration) -> Self {
        Self {
            max_attempts,
            initial_delay,
            max_delay,
        }
    }
}

pub const EXTERNAL_HTTP_RETRY: RetryPolicy =
    RetryPolicy::new(3, Duration::from_millis(250), Duration::from_secs(2));
pub const STORAGE_RETRY: RetryPolicy =
    RetryPolicy::new(3, Duration::from_millis(200), Duration::from_secs(1));

pub async fn retry_transient<T, E, Fut, Op, IsTransient>(
    policy: RetryPolicy,
    operation_name: &'static str,
    mut operation: Op,
    is_transient: IsTransient,
) -> Result<T, E>
where
    E: std::fmt::Debug,
    Fut: Future<Output = Result<T, E>>,
    IsTransient: Fn(&E) -> bool,
    Op: FnMut(usize) -> Fut,
{
    let max_attempts = policy.max_attempts.max(1);
    let mut attempt = 1;
    let mut delay = policy.initial_delay;

    loop {
        match operation(attempt).await {
            Ok(value) => return Ok(value),
            Err(error) if attempt < max_attempts && is_transient(&error) => {
                warn!(
                    operation_name,
                    attempt,
                    max_attempts,
                    retry_delay_ms = delay.as_millis(),
                    error = ?error,
                    "transient operation failed; retrying"
                );
                sleep(delay).await;
                attempt += 1;
                delay = next_delay(delay, policy.max_delay);
            }
            Err(error) => return Err(error),
        }
    }
}

pub fn is_transient_external_error(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        let message = cause.to_string().to_ascii_lowercase();

        message.contains("timed out")
            || message.contains("timeout")
            || message.contains("connection")
            || message.contains("connect")
            || message.contains("dns")
            || message.contains("temporarily unavailable")
            || retryable_status_in_message(&message)
    })
}

pub fn is_retryable_status_code(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::REQUEST_TIMEOUT
        || status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || status.is_server_error()
}

fn retryable_status_in_message(message: &str) -> bool {
    [
        "status 408",
        "status 429",
        "status 500",
        "status 502",
        "status 503",
        "status 504",
        "status 520",
        "status 521",
        "status 522",
        "status 523",
        "status 524",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

fn next_delay(current: Duration, max_delay: Duration) -> Duration {
    current.saturating_mul(2).min(max_delay)
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        time::Duration,
    };

    use super::{is_transient_external_error, retry_transient, RetryPolicy};

    #[tokio::test]
    async fn retries_transient_errors_until_success() -> anyhow::Result<()> {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_for_operation = Arc::clone(&attempts);
        let result = retry_transient(
            RetryPolicy::new(3, Duration::from_millis(1), Duration::from_millis(1)),
            "test operation",
            move |_| {
                let attempts = Arc::clone(&attempts_for_operation);
                async move {
                    let current = attempts.fetch_add(1, Ordering::SeqCst) + 1;

                    if current < 3 {
                        anyhow::bail!("request failed with status 503");
                    }

                    Ok("ok")
                }
            },
            is_transient_external_error,
        )
        .await?;

        assert_eq!(result, "ok");
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
        Ok(())
    }

    #[tokio::test]
    async fn does_not_retry_permanent_errors() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_for_operation = Arc::clone(&attempts);
        let result: anyhow::Result<&'static str> = retry_transient(
            RetryPolicy::new(3, Duration::from_millis(1), Duration::from_millis(1)),
            "test operation",
            move |_| {
                let attempts = Arc::clone(&attempts_for_operation);
                async move {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    anyhow::bail!("request failed with status 400")
                }
            },
            is_transient_external_error,
        )
        .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }
}
