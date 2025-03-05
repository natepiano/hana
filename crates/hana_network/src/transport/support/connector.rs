use std::{fmt::Debug, time::Duration};

use error_stack::Report;
use tracing::debug;

use crate::prelude::*;

pub const DEFAULT_MAX_ATTEMPTS: u8 = 15;
pub const DEFAULT_RETRY_DELAY: Duration = Duration::from_millis(200);

/// Configuration for connection retries
#[derive(Debug, Clone, Copy)]
pub struct RetryConfig {
    pub max_attempts: u8,
    pub retry_delay:  Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            retry_delay:  DEFAULT_RETRY_DELAY,
        }
    }
}

/// Helper function to handle async connection attempts with custom retry configuration
pub async fn connect_with_retry<T, E, F, Fut>(
    connect_fn: F,
    config: RetryConfig,
    context: impl Debug,
) -> Result<T>
where
    F: Fn() -> Fut + Send,
    Fut: std::future::Future<Output = std::result::Result<T, E>> + Send,
    E: Debug,
{
    let mut attempts = 0;
    loop {
        match connect_fn().await {
            Ok(connection) => return Ok(connection),
            Err(err) => {
                attempts += 1;
                if attempts >= config.max_attempts {
                    return Err(Report::new(Error::ConnectionTimeout)
                        .attach_printable(format!("Failed to connect after {attempts} attempts"))
                        .attach_printable(format!("Last error: {:?}", err)));
                }
                debug!(
                    "Connection attempt {} for {:?} failed, retrying...",
                    attempts, context
                );
                tokio::time::sleep(config.retry_delay).await;
            }
        }
    }
}
