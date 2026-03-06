// Copyright (c) 2025-present Don Johnson
// Licensed under the Business Source License 1.1 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://github.com/copyleftdev/gashammer/blob/main/LICENSE
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// GasHammer is a registered trademark of Don Johnson.

//! Telemetry pipeline — event log, batching, and shipping.
//!
//! The pipeline consists of:
//! 1. An `EventLog` ring buffer for edge-local event storage
//! 2. A `Shipper` that batches events and sends to hive
//!
//! Ref: RFC-0006 §3–4.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::event::TelemetryEvent;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Default ring buffer capacity (1M events).
pub const DEFAULT_BUFFER_CAPACITY: usize = 1_000_000;

/// Default batch size for shipping.
pub const DEFAULT_BATCH_SIZE: usize = 1_000;

/// Default flush interval in milliseconds.
pub const DEFAULT_FLUSH_INTERVAL_MS: u64 = 100;

/// Configuration for the telemetry pipeline.
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Ring buffer capacity (number of events).
    pub buffer_capacity: usize,
    /// Maximum batch size for shipping.
    pub batch_size: usize,
    /// Flush interval in milliseconds.
    pub flush_interval_ms: u64,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            buffer_capacity: DEFAULT_BUFFER_CAPACITY,
            batch_size: DEFAULT_BATCH_SIZE,
            flush_interval_ms: DEFAULT_FLUSH_INTERVAL_MS,
        }
    }
}

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

/// Atomic counters for the telemetry pipeline.
#[derive(Debug, Default)]
pub struct PipelineMetrics {
    pub gashammer_events_generated: AtomicU64,
    pub gashammer_events_shipped: AtomicU64,
    pub gashammer_events_dropped: AtomicU64,
    pub gashammer_batches_shipped: AtomicU64,
}

impl PipelineMetrics {
    pub fn events_generated(&self) -> u64 {
        self.gashammer_events_generated.load(Ordering::Relaxed)
    }

    pub fn events_shipped(&self) -> u64 {
        self.gashammer_events_shipped.load(Ordering::Relaxed)
    }

    pub fn events_dropped(&self) -> u64 {
        self.gashammer_events_dropped.load(Ordering::Relaxed)
    }

    pub fn batches_shipped(&self) -> u64 {
        self.gashammer_batches_shipped.load(Ordering::Relaxed)
    }

    /// Buffer utilization as a fraction (0.0 to 1.0).
    pub fn buffer_utilization(&self, buffer_len: u64, capacity: u64) -> f64 {
        if capacity == 0 {
            return 0.0;
        }
        buffer_len as f64 / capacity as f64
    }
}

// ---------------------------------------------------------------------------
// Event log (bounded channel as ring buffer)
// ---------------------------------------------------------------------------

/// Edge-local event log backed by a bounded channel.
///
/// When the buffer is full, the oldest event is dropped (by trying to
/// receive one before sending). The dropped counter tracks how many
/// events were discarded due to backpressure.
pub struct EventLog {
    tx: mpsc::Sender<TelemetryEvent>,
    rx: tokio::sync::Mutex<mpsc::Receiver<TelemetryEvent>>,
    metrics: Arc<PipelineMetrics>,
    capacity: usize,
}

impl EventLog {
    /// Create a new event log with the given capacity.
    pub fn new(capacity: usize, metrics: Arc<PipelineMetrics>) -> Self {
        let (tx, rx) = mpsc::channel(capacity);
        Self {
            tx,
            rx: tokio::sync::Mutex::new(rx),
            metrics,
            capacity,
        }
    }

    /// Push an event into the log. If the buffer is full, drops the
    /// oldest event and increments the drop counter.
    pub async fn push(&self, event: TelemetryEvent) {
        self.metrics
            .gashammer_events_generated
            .fetch_add(1, Ordering::Relaxed);

        // Try to send. If full, force-drain one and retry.
        match self.tx.try_send(event) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(event)) => {
                // Drop oldest event
                {
                    let mut rx = self.rx.lock().await;
                    let _ = rx.try_recv();
                }
                self.metrics
                    .gashammer_events_dropped
                    .fetch_add(1, Ordering::Relaxed);
                // Retry — should succeed now
                let _ = self.tx.try_send(event);
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                self.metrics
                    .gashammer_events_dropped
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Drain up to `max` events from the buffer.
    pub async fn drain(&self, max: usize) -> Vec<TelemetryEvent> {
        let mut batch = Vec::with_capacity(max);
        let mut rx = self.rx.lock().await;
        while batch.len() < max {
            match rx.try_recv() {
                Ok(event) => batch.push(event),
                Err(_) => break,
            }
        }
        batch
    }

    /// Current number of pending events (approximate).
    pub fn pending_approx(&self) -> usize {
        // mpsc channel doesn't expose len(), so we track via metrics
        let generated = self.metrics.events_generated();
        let shipped = self.metrics.events_shipped();
        let dropped = self.metrics.events_dropped();
        (generated.saturating_sub(shipped).saturating_sub(dropped)) as usize
    }

    /// Buffer capacity.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Reference to pipeline metrics.
    pub fn metrics(&self) -> &Arc<PipelineMetrics> {
        &self.metrics
    }
}

// ---------------------------------------------------------------------------
// Event batch
// ---------------------------------------------------------------------------

/// A batch of events ready for shipping.
#[derive(Debug, Clone)]
pub struct EventBatch {
    /// Events in this batch.
    pub events: Vec<TelemetryEvent>,
    /// Number of events dropped since the last batch.
    pub dropped_since_last: u64,
}

// ---------------------------------------------------------------------------
// Shipper
// ---------------------------------------------------------------------------

/// Event sink — receives batches for processing.
///
/// Implementations handle shipping to hive (gRPC), local storage, etc.
pub trait EventSink: Send + Sync + 'static {
    /// Ship a batch of events. Returns the number of events accepted.
    fn ship(
        &self,
        batch: EventBatch,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64, String>> + Send + '_>>;
}

/// A simple channel-based sink for testing and local collection.
pub struct ChannelSink {
    tx: mpsc::Sender<EventBatch>,
}

impl ChannelSink {
    pub fn new(tx: mpsc::Sender<EventBatch>) -> Self {
        Self { tx }
    }
}

impl EventSink for ChannelSink {
    fn ship(
        &self,
        batch: EventBatch,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64, String>> + Send + '_>> {
        Box::pin(async move {
            let count = batch.events.len() as u64;
            self.tx
                .send(batch)
                .await
                .map_err(|_| "sink channel closed".to_string())?;
            Ok(count)
        })
    }
}

/// The shipper drains events from the EventLog and ships them via an EventSink.
///
/// It runs as a background tokio task, flushing every `flush_interval_ms`
/// or when a batch reaches `batch_size`.
pub struct Shipper {
    log: Arc<EventLog>,
    config: PipelineConfig,
    metrics: Arc<PipelineMetrics>,
}

impl Shipper {
    /// Create a new shipper for the given event log.
    pub fn new(log: Arc<EventLog>, config: PipelineConfig) -> Self {
        let metrics = Arc::clone(log.metrics());
        Self {
            log,
            config,
            metrics,
        }
    }

    /// Start the shipper as a background task.
    pub fn start(self, sink: Box<dyn EventSink>) -> JoinHandle<()> {
        let log = self.log;
        let config = self.config;
        let metrics = self.metrics;

        tokio::spawn(async move {
            let mut last_dropped = 0u64;
            let mut ticker = tokio::time::interval(Duration::from_millis(config.flush_interval_ms));

            loop {
                ticker.tick().await;

                let events = log.drain(config.batch_size).await;
                if events.is_empty() {
                    continue;
                }

                let current_dropped = metrics.events_dropped();
                let dropped_since_last = current_dropped.saturating_sub(last_dropped);
                last_dropped = current_dropped;

                let count = events.len() as u64;
                let batch = EventBatch {
                    events,
                    dropped_since_last,
                };

                match sink.ship(batch).await {
                    Ok(accepted) => {
                        metrics
                            .gashammer_events_shipped
                            .fetch_add(accepted, Ordering::Relaxed);
                        metrics
                            .gashammer_batches_shipped
                            .fetch_add(1, Ordering::Relaxed);
                        tracing::trace!(events = count, accepted, "telemetry batch shipped");
                    }
                    Err(err) => {
                        tracing::warn!(
                            events = count,
                            error = %err,
                            "telemetry batch shipping failed"
                        );
                        // Events are lost — counted as dropped
                        metrics
                            .gashammer_events_dropped
                            .fetch_add(count, Ordering::Relaxed);
                    }
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{EventPayload, TelemetryEvent};
    use uuid::Uuid;

    fn test_event(seq: u64) -> TelemetryEvent {
        TelemetryEvent::new(
            Uuid::nil(),
            Uuid::nil(),
            0,
            seq * 1_000_000,
            1_700_000_000_000_000_000 + seq,
            EventPayload::TxSubmitted {
                tx_hash: format!("0x{seq:064x}"),
                gas_limit: 21_000,
                template_name: "simple-transfer".into(),
                account: "0x0".into(),
                nonce: seq,
            },
        )
    }

    #[tokio::test]
    async fn test_event_log_push_and_drain() {
        let metrics = Arc::new(PipelineMetrics::default());
        let log = EventLog::new(100, metrics.clone());

        log.push(test_event(0)).await;
        log.push(test_event(1)).await;
        log.push(test_event(2)).await;

        let batch = log.drain(10).await;
        assert_eq!(batch.len(), 3);
        assert_eq!(metrics.events_generated(), 3);
    }

    #[tokio::test]
    async fn test_event_log_drain_respects_max() {
        let metrics = Arc::new(PipelineMetrics::default());
        let log = EventLog::new(100, metrics);

        for i in 0..10 {
            log.push(test_event(i)).await;
        }

        let batch = log.drain(5).await;
        assert_eq!(batch.len(), 5);

        let rest = log.drain(100).await;
        assert_eq!(rest.len(), 5);
    }

    #[tokio::test]
    async fn test_event_log_overflow_drops_oldest() {
        let metrics = Arc::new(PipelineMetrics::default());
        let log = EventLog::new(3, metrics.clone());

        // Fill buffer
        log.push(test_event(0)).await;
        log.push(test_event(1)).await;
        log.push(test_event(2)).await;

        // Overflow — should drop oldest
        log.push(test_event(3)).await;

        assert_eq!(metrics.events_generated(), 4);
        assert_eq!(metrics.events_dropped(), 1);

        // Should have events 1, 2, 3 (event 0 was dropped)
        let batch = log.drain(10).await;
        assert_eq!(batch.len(), 3);
    }

    #[tokio::test]
    async fn test_event_log_empty_drain() {
        let metrics = Arc::new(PipelineMetrics::default());
        let log = EventLog::new(100, metrics);

        let batch = log.drain(10).await;
        assert!(batch.is_empty());
    }

    #[tokio::test]
    async fn test_shipper_ships_events() {
        let metrics = Arc::new(PipelineMetrics::default());
        let log = Arc::new(EventLog::new(100, metrics.clone()));

        // Push some events
        for i in 0..5 {
            log.push(test_event(i)).await;
        }

        let config = PipelineConfig {
            buffer_capacity: 100,
            batch_size: 10,
            flush_interval_ms: 20,
        };

        let (batch_tx, mut batch_rx) = mpsc::channel(10);
        let sink = Box::new(ChannelSink::new(batch_tx));
        let shipper = Shipper::new(Arc::clone(&log), config);
        let handle = shipper.start(sink);

        // Wait for shipper to flush
        tokio::time::sleep(Duration::from_millis(50)).await;

        let batch = batch_rx.try_recv().expect("should have received batch");
        assert_eq!(batch.events.len(), 5);
        assert_eq!(metrics.events_shipped(), 5);
        assert_eq!(metrics.batches_shipped(), 1);

        handle.abort();
    }

    #[tokio::test]
    async fn test_pipeline_metrics_default() {
        let metrics = PipelineMetrics::default();
        assert_eq!(metrics.events_generated(), 0);
        assert_eq!(metrics.events_shipped(), 0);
        assert_eq!(metrics.events_dropped(), 0);
        assert_eq!(metrics.batches_shipped(), 0);
    }

    #[tokio::test]
    async fn test_buffer_utilization_calc() {
        let metrics = PipelineMetrics::default();
        let util = metrics.buffer_utilization(500, 1000);
        assert!((util - 0.5).abs() < 0.001);

        let util_zero = metrics.buffer_utilization(0, 0);
        assert!((util_zero).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_pipeline_config_defaults() {
        let config = PipelineConfig::default();
        assert_eq!(config.buffer_capacity, DEFAULT_BUFFER_CAPACITY);
        assert_eq!(config.batch_size, DEFAULT_BATCH_SIZE);
        assert_eq!(config.flush_interval_ms, DEFAULT_FLUSH_INTERVAL_MS);
    }
}
