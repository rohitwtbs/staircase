//! Async runtime scaffolding: per-device polling, graceful shutdown, and task
//! supervision.
//!
//! These helpers are protocol-agnostic. The gateway layer composes them with
//! concrete [`ProtocolDriver`](crate::traits::ProtocolDriver)s.
//!
//! Graceful shutdown is modeled with a [`CancellationToken`]: clone it into each
//! task and call [`CancellationToken::cancel`] to stop them all.

use std::future::Future;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use tokio::time::{MissedTickBehavior, interval};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::error::{Result, StaircaseError};
use crate::model::DataPoint;
use crate::observability::Metrics;
use crate::traits::ProtocolDriver;

/// Configuration for [`supervise`].
#[derive(Debug, Clone)]
pub struct SupervisorConfig {
    /// Maximum number of restarts before giving up (`None` = unlimited).
    pub max_restarts: Option<u32>,
    /// Delay between a failure and the next restart attempt.
    pub restart_backoff: Duration,
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            max_restarts: None,
            restart_backoff: Duration::from_secs(1),
        }
    }
}

/// Run a task to completion, restarting it on error until it succeeds, the
/// cancellation token fires, or `max_restarts` is exceeded.
///
/// `factory` is called to (re)create the task future on each attempt.
///
/// Returns:
/// - `Ok(())` if the task completed successfully, or shutdown was requested via
///   the cancellation token.
/// - `Err(StaircaseError::Other(..))` if the restart budget
///   (`SupervisorConfig::max_restarts`) was exhausted; the error message
///   includes the task name, the restart count, and the last failure.
pub async fn supervise<F, Fut>(
    name: impl Into<String>,
    token: CancellationToken,
    config: SupervisorConfig,
    mut factory: F,
) -> Result<()>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<()>>,
{
    let name = name.into();
    let mut restarts: u32 = 0;

    loop {
        if token.is_cancelled() {
            return Ok(());
        }

        tokio::select! {
            _ = token.cancelled() => return Ok(()),
            result = factory() => match result {
                Ok(()) => {
                    info!(task = %name, "supervised task completed");
                    return Ok(());
                }
                Err(e) => {
                    error!(task = %name, error = %e, "supervised task failed");
                    restarts += 1;
                    if let Some(max) = config.max_restarts {
                        if restarts > max {
                            warn!(task = %name, restarts, "max restarts exceeded; giving up");
                            return Err(StaircaseError::Other(anyhow::anyhow!(
                                "supervised task '{name}' permanently failed after {restarts} restart(s); last error: {e}"
                            )));
                        }
                    }
                    tokio::select! {
                        _ = token.cancelled() => return Ok(()),
                        _ = tokio::time::sleep(config.restart_backoff) => {}
                    }
                }
            }
        }
    }
}

/// Continuously poll a single device until cancelled.
///
/// Connects the driver, then on every tick of `period` polls it and forwards the
/// resulting [`DataPoint`]s through `tx`. Latency, throughput, and protocol
/// errors are recorded in `metrics`. Returns `Ok(())` on graceful shutdown or
/// when the receiver is dropped.
pub async fn poll_device(
    mut driver: Box<dyn ProtocolDriver>,
    period: Duration,
    token: CancellationToken,
    tx: mpsc::Sender<DataPoint>,
    metrics: std::sync::Arc<Metrics>,
) -> Result<()> {
    driver.connect().await?;

    let mut ticker = interval(period);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = token.cancelled() => {
                let _ = driver.disconnect().await;
                break;
            }
            _ = ticker.tick() => {
                let start = Instant::now();
                match driver.poll().await {
                    Ok(points) => {
                        metrics.record_poll_latency(start.elapsed().as_millis() as u64);
                        let count = points.len() as u64;
                        for p in points {
                            if tx.send(p).await.is_err() {
                                // Receiver gone: shut down gracefully.
                                let _ = driver.disconnect().await;
                                return Ok(());
                            }
                        }
                        metrics.add_throughput(count);
                    }
                    Err(e) => {
                        metrics.inc_protocol_error();
                        warn!(error = %e, protocol = driver.protocol(), "device poll failed");
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::MockDriver;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn poll_device_forwards_points_and_stops_on_cancel() {
        let token = CancellationToken::new();
        let (tx, mut rx) = mpsc::channel(16);
        let metrics = Metrics::new();
        let driver = Box::new(MockDriver::new("dev_1"));

        let handle = tokio::spawn(poll_device(
            driver,
            Duration::from_millis(10),
            token.clone(),
            tx,
            metrics.clone(),
        ));

        let first = rx.recv().await.expect("a data point");
        assert_eq!(first.device_id, "dev_1");

        token.cancel();
        handle.await.unwrap().unwrap();
        assert!(metrics.snapshot().poll_count >= 1);
    }

    #[tokio::test]
    async fn supervise_restarts_until_success() {
        let token = CancellationToken::new();
        let attempts = Arc::new(AtomicU32::new(0));
        let a = attempts.clone();

        supervise(
            "flaky",
            token,
            SupervisorConfig {
                max_restarts: Some(5),
                restart_backoff: Duration::from_millis(1),
            },
            move || {
                let a = a.clone();
                async move {
                    let n = a.fetch_add(1, Ordering::SeqCst);
                    if n < 2 {
                        Err(crate::error::StaircaseError::protocol("boom"))
                    } else {
                        Ok(())
                    }
                }
            },
        )
        .await
        .unwrap();

        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn supervise_returns_err_when_restart_budget_exhausted() {
        let token = CancellationToken::new();
        let result = supervise(
            "always_fails",
            token,
            SupervisorConfig {
                max_restarts: Some(2),
                restart_backoff: Duration::from_millis(1),
            },
            || async { Err(crate::error::StaircaseError::protocol("boom")) },
        )
        .await;

        let err = result.expect_err("exhausted restart budget should return Err");
        let msg = err.to_string();
        assert!(msg.contains("always_fails"));
        assert!(msg.contains("boom"));
    }

    #[tokio::test]
    async fn supervise_returns_ok_on_cancellation() {
        let token = CancellationToken::new();
        token.cancel();
        let result = supervise(
            "cancelled",
            token,
            SupervisorConfig {
                max_restarts: Some(0),
                restart_backoff: Duration::from_millis(1),
            },
            || async { Err(crate::error::StaircaseError::protocol("boom")) },
        )
        .await;
        assert!(result.is_ok());
    }
}
