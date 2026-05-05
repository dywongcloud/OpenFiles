//! Background sync helpers for the OpenFiles engine.
//!
//! AWS S3 Files exports local file mutations to the bucket in batches. The
//! equivalent OpenFiles behavior is represented by periodically calling
//! [`OpenFilesEngine::flush`]. In production deployments this task is normally
//! owned by the CLI daemon, HTTP gateway, FUSE mount process, or wasmCloud host
//! plugin.

use crate::{OpenFilesEngine, Result};
use std::time::Duration;
use tokio::{task::JoinHandle, time};

#[derive(Clone, Debug)]
pub struct BackgroundSyncConfig {
    pub flush_interval: Duration,
    pub expire_interval: Duration,
}

impl Default for BackgroundSyncConfig {
    fn default() -> Self {
        Self {
            flush_interval: Duration::from_secs(60),
            expire_interval: Duration::from_secs(60 * 60),
        }
    }
}

pub fn spawn_background_sync(
    engine: OpenFilesEngine,
    cfg: BackgroundSyncConfig,
) -> JoinHandle<Result<()>> {
    tokio::spawn(async move {
        let mut flush_tick = time::interval(cfg.flush_interval);
        let mut expire_tick = time::interval(cfg.expire_interval);
        loop {
            tokio::select! {
                _ = flush_tick.tick() => {
                    let flushed = engine.flush().await?;
                    if flushed > 0 {
                        tracing::info!(flushed, "exported cached changes to object backend");
                    }
                }
                _ = expire_tick.tick() => {
                    let expired = engine.expire_cache().await?;
                    if expired > 0 {
                        tracing::info!(expired, "expired inactive cached objects");
                    }
                }
            }
        }
    })
}

pub async fn flush_once(engine: &OpenFilesEngine) -> Result<usize> {
    engine.flush().await
}

pub async fn expire_once(engine: &OpenFilesEngine) -> Result<u64> {
    engine.expire_cache().await
}
