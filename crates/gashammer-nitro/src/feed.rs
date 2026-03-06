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

use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use crate::error::NitroError;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the Nitro sequencer feed consumer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedConfig {
    /// WebSocket URL for the feed relay (e.g. `ws://relay:9642`).
    pub url: String,
    /// Channel buffer size for parsed feed events.
    #[serde(default = "default_channel_buffer")]
    pub channel_buffer: usize,
    /// Ping interval in milliseconds.
    #[serde(default = "default_ping_interval_ms")]
    pub ping_interval_ms: u64,
    /// Pong timeout in milliseconds — disconnect if no pong received within this window.
    #[serde(default = "default_pong_timeout_ms")]
    pub pong_timeout_ms: u64,
    /// Stall detection threshold in milliseconds — emit warning if no message for this long.
    #[serde(default = "default_stall_threshold_ms")]
    pub stall_threshold_ms: u64,
}

fn default_channel_buffer() -> usize {
    4096
}

fn default_ping_interval_ms() -> u64 {
    10_000
}

fn default_pong_timeout_ms() -> u64 {
    5_000
}

fn default_stall_threshold_ms() -> u64 {
    1_000
}

impl FeedConfig {
    pub fn ping_interval(&self) -> Duration {
        Duration::from_millis(self.ping_interval_ms)
    }

    pub fn pong_timeout(&self) -> Duration {
        Duration::from_millis(self.pong_timeout_ms)
    }

    pub fn stall_threshold(&self) -> Duration {
        Duration::from_millis(self.stall_threshold_ms)
    }
}

// ---------------------------------------------------------------------------
// Wire types — Nitro feed JSON schema (RFC-0001 §4.2)
// ---------------------------------------------------------------------------

/// Top-level broadcast envelope from the feed relay.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BroadcastMessage {
    pub version: Option<i32>,
    #[serde(default)]
    pub messages: Vec<BroadcastFeedMessage>,
    pub confirmed_sequence_number_message: Option<ConfirmedSequenceNumberMessage>,
}

/// A single feed message containing one sequenced L2 message.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BroadcastFeedMessage {
    pub sequence_number: u64,
    pub message: Option<MessageWithMetadata>,
    pub signature: Option<String>,
}

/// Wrapper around an L1 incoming message with delayed-messages-read count.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageWithMetadata {
    pub message: Option<L1IncomingMessage>,
    pub delayed_messages_read: Option<u64>,
}

/// The L1 incoming message containing the actual L2 transaction data.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct L1IncomingMessage {
    pub header: Option<L1IncomingMessageHeader>,
    /// Encoded L2 transaction(s) — hex-encoded bytes.
    #[serde(default)]
    pub l2_msg: Option<String>,
    pub batch_gas_cost: Option<u64>,
}

/// Header for an L1 incoming message.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct L1IncomingMessageHeader {
    /// Message kind: 3 = sequencer message.
    pub kind: Option<u8>,
    pub sender: Option<String>,
    pub block_number: Option<u64>,
    pub timestamp: Option<u64>,
    pub request_id: Option<String>,
    pub base_fee_l1: Option<String>,
}

/// Confirmed sequence number notification.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmedSequenceNumberMessage {
    pub sequence_number: u64,
}

// ---------------------------------------------------------------------------
// Parsed feed event (emitted to consumers)
// ---------------------------------------------------------------------------

/// Events emitted by the feed consumer to downstream processors.
#[derive(Debug, Clone)]
pub enum FeedEvent {
    /// A new sequenced message was received.
    Message {
        sequence_number: u64,
        timestamp: Instant,
        l2_msg: Option<String>,
        l1_block_number: Option<u64>,
        l1_timestamp: Option<u64>,
    },
    /// A sequence number gap was detected.
    SequenceGap {
        expected: u64,
        received: u64,
        timestamp: Instant,
    },
    /// A confirmed sequence number was received.
    Confirmed {
        sequence_number: u64,
        timestamp: Instant,
    },
    /// The feed stalled (no messages for longer than the threshold).
    Stall {
        duration: Duration,
        timestamp: Instant,
    },
    /// The feed disconnected.
    Disconnected { reason: String, timestamp: Instant },
    /// The feed reconnected.
    Reconnected { attempt: u32, timestamp: Instant },
}

// ---------------------------------------------------------------------------
// Metrics counters (in-memory, read by telemetry layer)
// ---------------------------------------------------------------------------

/// Feed consumer metrics for observability.
#[derive(Debug, Default)]
pub struct FeedMetrics {
    pub messages_received: std::sync::atomic::AtomicU64,
    pub parse_errors: std::sync::atomic::AtomicU64,
    pub reconnections: std::sync::atomic::AtomicU64,
    pub sequence_gaps: std::sync::atomic::AtomicU64,
}

impl FeedMetrics {
    pub fn new() -> Self {
        Self::default()
    }
}

// ---------------------------------------------------------------------------
// Reconnection backoff
// ---------------------------------------------------------------------------

/// Exponential backoff for feed reconnection: 100ms → 200ms → 400ms → ... → cap.
struct ReconnectBackoff {
    current: Duration,
    cap: Duration,
}

impl ReconnectBackoff {
    fn new() -> Self {
        Self {
            current: Duration::from_millis(100),
            cap: Duration::from_secs(10),
        }
    }

    fn next_delay(&mut self) -> Duration {
        let delay = self.current;
        self.current = (self.current * 2).min(self.cap);
        delay
    }

    fn reset(&mut self) {
        self.current = Duration::from_millis(100);
    }
}

// ---------------------------------------------------------------------------
// Feed consumer
// ---------------------------------------------------------------------------

/// Nitro sequencer feed consumer.
///
/// Connects to a feed relay via WebSocket, parses `BroadcastFeedMessage`
/// frames, and emits [`FeedEvent`]s on an mpsc channel.
pub struct NitroFeedConsumer {
    config: FeedConfig,
    metrics: std::sync::Arc<FeedMetrics>,
}

impl NitroFeedConsumer {
    /// Create a new feed consumer.
    pub fn new(config: FeedConfig) -> Self {
        Self {
            config,
            metrics: std::sync::Arc::new(FeedMetrics::new()),
        }
    }

    /// Returns a reference to the live metrics.
    pub fn metrics(&self) -> &std::sync::Arc<FeedMetrics> {
        &self.metrics
    }

    /// Returns a reference to the config.
    pub fn config(&self) -> &FeedConfig {
        &self.config
    }

    /// Start the feed consumer. Returns a channel receiver for feed events
    /// and a `JoinHandle` for the background task.
    ///
    /// The consumer will automatically reconnect on disconnection with
    /// exponential backoff.
    pub fn start(
        &self,
    ) -> (
        mpsc::Receiver<FeedEvent>,
        tokio::task::JoinHandle<Result<(), NitroError>>,
    ) {
        let (tx, rx) = mpsc::channel(self.config.channel_buffer);
        let config = self.config.clone();
        let metrics = self.metrics.clone();

        let handle = tokio::spawn(async move { feed_loop(config, tx, metrics).await });

        (rx, handle)
    }
}

impl std::fmt::Debug for NitroFeedConsumer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NitroFeedConsumer")
            .field("url", &self.config.url)
            .finish()
    }
}

/// Main feed loop with reconnection logic.
async fn feed_loop(
    config: FeedConfig,
    tx: mpsc::Sender<FeedEvent>,
    metrics: std::sync::Arc<FeedMetrics>,
) -> Result<(), NitroError> {
    let mut backoff = ReconnectBackoff::new();
    let mut last_sequence_number: Option<u64> = None;
    let mut attempt: u32 = 0;

    loop {
        match connect_and_consume(&config, &tx, &metrics, &mut last_sequence_number).await {
            Ok(()) => {
                // Clean shutdown requested — exit the loop.
                tracing::info!("feed consumer shut down cleanly");
                return Ok(());
            }
            Err(e) => {
                attempt += 1;
                metrics
                    .reconnections
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                let _ = tx
                    .send(FeedEvent::Disconnected {
                        reason: e.to_string(),
                        timestamp: Instant::now(),
                    })
                    .await;

                let delay = backoff.next_delay();
                tracing::warn!(
                    attempt,
                    delay_ms = delay.as_millis() as u64,
                    error = %e,
                    "feed disconnected, reconnecting"
                );

                tokio::time::sleep(delay).await;

                // Reset backoff after a successful reconnect delivers messages.
                if last_sequence_number.is_some() {
                    backoff.reset();
                }

                let _ = tx
                    .send(FeedEvent::Reconnected {
                        attempt,
                        timestamp: Instant::now(),
                    })
                    .await;
            }
        }

        if attempt > 100 {
            tracing::error!("feed consumer exceeded 100 reconnection attempts");
            return Err(NitroError::Feed(
                "exceeded maximum reconnection attempts".into(),
            ));
        }
    }
}

/// Connect to the feed relay and consume messages until disconnection.
async fn connect_and_consume(
    config: &FeedConfig,
    tx: &mpsc::Sender<FeedEvent>,
    metrics: &std::sync::Arc<FeedMetrics>,
    last_sequence_number: &mut Option<u64>,
) -> Result<(), NitroError> {
    // Build WS request with User-Agent and DNA headers.
    let ws_request = crate::headers::ws_request(&config.url)?;

    let (ws_stream, _response) = tokio_tungstenite::connect_async(ws_request)
        .await
        .map_err(|e| NitroError::Feed(format!("WebSocket connect failed: {e}")))?;

    tracing::info!(url = %config.url, "connected to feed relay");

    let (mut ws_sink, mut ws_stream) = ws_stream.split();
    let mut last_message_time = Instant::now();
    let mut ping_interval = tokio::time::interval(config.ping_interval());
    // Skip the first immediate tick.
    ping_interval.tick().await;

    loop {
        tokio::select! {
            // Incoming WebSocket message
            msg = ws_stream.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        last_message_time = Instant::now();
                        process_text_message(
                            &text,
                            tx,
                            metrics,
                            last_sequence_number,
                        )
                        .await;
                    }
                    Some(Ok(Message::Ping(data))) => {
                        let _ = ws_sink.send(Message::Pong(data)).await;
                    }
                    Some(Ok(Message::Pong(_))) => {
                        // Pong received — connection is alive.
                    }
                    Some(Ok(Message::Close(_))) => {
                        return Err(NitroError::Feed("server sent close frame".into()));
                    }
                    Some(Ok(Message::Binary(_))) => {
                        // Unexpected binary frame — skip.
                        tracing::debug!("received unexpected binary frame");
                    }
                    Some(Ok(Message::Frame(_))) => {
                        // Raw frame — skip.
                    }
                    Some(Err(e)) => {
                        return Err(NitroError::Feed(format!("WebSocket error: {e}")));
                    }
                    None => {
                        return Err(NitroError::Feed("WebSocket stream ended".into()));
                    }
                }
            }

            // Periodic ping
            _ = ping_interval.tick() => {
                // Check for stall
                let since_last = last_message_time.elapsed();
                if since_last >= config.stall_threshold() {
                    let _ = tx
                        .send(FeedEvent::Stall {
                            duration: since_last,
                            timestamp: Instant::now(),
                        })
                        .await;
                    tracing::warn!(
                        stall_secs = since_last.as_secs_f64(),
                        "feed stall detected"
                    );
                }

                // Send ping
                if let Err(e) = ws_sink.send(Message::Ping(vec![])).await {
                    return Err(NitroError::Feed(format!("ping send failed: {e}")));
                }
            }
        }
    }
}

/// Parse a text frame as a BroadcastMessage and emit events.
async fn process_text_message(
    text: &str,
    tx: &mpsc::Sender<FeedEvent>,
    metrics: &std::sync::Arc<FeedMetrics>,
    last_sequence_number: &mut Option<u64>,
) {
    let broadcast: BroadcastMessage = match serde_json::from_str(text) {
        Ok(b) => b,
        Err(e) => {
            metrics
                .parse_errors
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            tracing::warn!(error = %e, "failed to parse feed message");
            return;
        }
    };

    // Process each feed message
    for msg in &broadcast.messages {
        let now = Instant::now();
        metrics
            .messages_received
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // Sequence gap detection
        if let Some(last_seq) = *last_sequence_number {
            let expected = last_seq + 1;
            if msg.sequence_number != expected {
                metrics
                    .sequence_gaps
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                tracing::warn!(
                    expected,
                    received = msg.sequence_number,
                    "sequence number gap detected"
                );
                let _ = tx
                    .send(FeedEvent::SequenceGap {
                        expected,
                        received: msg.sequence_number,
                        timestamp: now,
                    })
                    .await;
            }
        }
        *last_sequence_number = Some(msg.sequence_number);

        // Extract fields from the nested message
        let (l2_msg, l1_block_number, l1_timestamp) = msg
            .message
            .as_ref()
            .and_then(|mwm| mwm.message.as_ref())
            .map(|l1m| {
                let header = l1m.header.as_ref();
                (
                    l1m.l2_msg.clone(),
                    header.and_then(|h| h.block_number),
                    header.and_then(|h| h.timestamp),
                )
            })
            .unwrap_or((None, None, None));

        let _ = tx
            .send(FeedEvent::Message {
                sequence_number: msg.sequence_number,
                timestamp: now,
                l2_msg,
                l1_block_number,
                l1_timestamp,
            })
            .await;
    }

    // Handle confirmed sequence number
    if let Some(confirmed) = &broadcast.confirmed_sequence_number_message {
        let _ = tx
            .send(FeedEvent::Confirmed {
                sequence_number: confirmed.sequence_number,
                timestamp: Instant::now(),
            })
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn test_feed_config_defaults() {
        let config = FeedConfig {
            url: "ws://localhost:9642".into(),
            channel_buffer: default_channel_buffer(),
            ping_interval_ms: default_ping_interval_ms(),
            pong_timeout_ms: default_pong_timeout_ms(),
            stall_threshold_ms: default_stall_threshold_ms(),
        };
        assert_eq!(config.channel_buffer, 4096);
        assert_eq!(config.ping_interval(), Duration::from_secs(10));
        assert_eq!(config.pong_timeout(), Duration::from_secs(5));
        assert_eq!(config.stall_threshold(), Duration::from_secs(1));
    }

    #[test]
    fn test_feed_config_serialization() {
        let config = FeedConfig {
            url: "ws://relay:9642".into(),
            channel_buffer: 2048,
            ping_interval_ms: 5000,
            pong_timeout_ms: 3000,
            stall_threshold_ms: 2000,
        };
        let json = serde_json::to_string(&config).expect("serialize");
        let parsed: FeedConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.url, config.url);
        assert_eq!(parsed.channel_buffer, 2048);
    }

    #[test]
    fn test_parse_broadcast_message_single() {
        let json = r#"{
            "version": 1,
            "messages": [{
                "sequenceNumber": 42,
                "message": {
                    "message": {
                        "header": {
                            "kind": 3,
                            "sender": "0xa4b1000000000000000000000000000073657175656e636572",
                            "blockNumber": 12345,
                            "timestamp": 1700000000,
                            "requestId": null,
                            "baseFeeL1": null
                        },
                        "l2Msg": "0xdeadbeef",
                        "batchGasCost": 21000
                    },
                    "delayedMessagesRead": 0
                },
                "signature": null
            }]
        }"#;

        let broadcast: BroadcastMessage = serde_json::from_str(json).expect("parse");
        assert_eq!(broadcast.version, Some(1));
        assert_eq!(broadcast.messages.len(), 1);

        let msg = &broadcast.messages[0];
        assert_eq!(msg.sequence_number, 42);

        let l1msg = msg.message.as_ref().unwrap().message.as_ref().unwrap();
        assert_eq!(l1msg.l2_msg.as_deref(), Some("0xdeadbeef"));
        assert_eq!(l1msg.batch_gas_cost, Some(21000));

        let header = l1msg.header.as_ref().unwrap();
        assert_eq!(header.kind, Some(3));
        assert_eq!(header.block_number, Some(12345));
        assert_eq!(header.timestamp, Some(1700000000));
    }

    #[test]
    fn test_parse_broadcast_message_multiple() {
        let json = r#"{
            "version": 1,
            "messages": [
                {"sequenceNumber": 100, "message": null, "signature": null},
                {"sequenceNumber": 101, "message": null, "signature": null},
                {"sequenceNumber": 102, "message": null, "signature": null}
            ]
        }"#;

        let broadcast: BroadcastMessage = serde_json::from_str(json).expect("parse");
        assert_eq!(broadcast.messages.len(), 3);
        assert_eq!(broadcast.messages[0].sequence_number, 100);
        assert_eq!(broadcast.messages[2].sequence_number, 102);
    }

    #[test]
    fn test_parse_broadcast_with_confirmed() {
        let json = r#"{
            "version": 1,
            "messages": [],
            "confirmedSequenceNumberMessage": {
                "sequenceNumber": 99
            }
        }"#;

        let broadcast: BroadcastMessage = serde_json::from_str(json).expect("parse");
        assert!(broadcast.messages.is_empty());
        let confirmed = broadcast.confirmed_sequence_number_message.unwrap();
        assert_eq!(confirmed.sequence_number, 99);
    }

    #[test]
    fn test_parse_corrupt_message() {
        let json = r#"not valid json at all"#;
        let result: Result<BroadcastMessage, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_empty_messages() {
        let json = r#"{"version": 1, "messages": []}"#;
        let broadcast: BroadcastMessage = serde_json::from_str(json).expect("parse");
        assert!(broadcast.messages.is_empty());
        assert!(broadcast.confirmed_sequence_number_message.is_none());
    }

    #[test]
    fn test_parse_minimal_message() {
        // Feed may send minimal messages without all optional fields.
        let json = r#"{
            "messages": [{
                "sequenceNumber": 1,
                "message": {
                    "message": {
                        "header": {
                            "kind": 3
                        }
                    }
                }
            }]
        }"#;

        let broadcast: BroadcastMessage = serde_json::from_str(json).expect("parse");
        assert_eq!(broadcast.messages.len(), 1);
        let header = broadcast.messages[0]
            .message
            .as_ref()
            .unwrap()
            .message
            .as_ref()
            .unwrap()
            .header
            .as_ref()
            .unwrap();
        assert_eq!(header.kind, Some(3));
        assert!(header.sender.is_none());
    }

    #[tokio::test]
    async fn test_process_text_message_basic() {
        let (tx, mut rx) = mpsc::channel(16);
        let metrics = std::sync::Arc::new(FeedMetrics::new());
        let mut last_seq = None;

        let json = r#"{
            "version": 1,
            "messages": [{
                "sequenceNumber": 42,
                "message": {
                    "message": {
                        "header": {"kind": 3, "blockNumber": 100, "timestamp": 1700000000},
                        "l2Msg": "0xabcd"
                    },
                    "delayedMessagesRead": 0
                },
                "signature": null
            }]
        }"#;

        process_text_message(json, &tx, &metrics, &mut last_seq).await;

        assert_eq!(metrics.messages_received.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.parse_errors.load(Ordering::Relaxed), 0);
        assert_eq!(last_seq, Some(42));

        let event = rx.try_recv().expect("should have event");
        match event {
            FeedEvent::Message {
                sequence_number,
                l2_msg,
                l1_block_number,
                l1_timestamp,
                ..
            } => {
                assert_eq!(sequence_number, 42);
                assert_eq!(l2_msg.as_deref(), Some("0xabcd"));
                assert_eq!(l1_block_number, Some(100));
                assert_eq!(l1_timestamp, Some(1700000000));
            }
            _ => panic!("expected Message event"),
        }
    }

    #[tokio::test]
    async fn test_process_text_message_sequence_gap() {
        let (tx, mut rx) = mpsc::channel(16);
        let metrics = std::sync::Arc::new(FeedMetrics::new());
        let mut last_seq = Some(40);

        let json = r#"{
            "version": 1,
            "messages": [{"sequenceNumber": 43, "message": null, "signature": null}]
        }"#;

        process_text_message(json, &tx, &metrics, &mut last_seq).await;

        assert_eq!(metrics.sequence_gaps.load(Ordering::Relaxed), 1);
        assert_eq!(last_seq, Some(43));

        // First event should be SequenceGap
        let event = rx.try_recv().expect("should have gap event");
        match event {
            FeedEvent::SequenceGap {
                expected, received, ..
            } => {
                assert_eq!(expected, 41);
                assert_eq!(received, 43);
            }
            _ => panic!("expected SequenceGap event"),
        }
    }

    #[tokio::test]
    async fn test_process_text_message_parse_error() {
        let (tx, _rx) = mpsc::channel(16);
        let metrics = std::sync::Arc::new(FeedMetrics::new());
        let mut last_seq = None;

        process_text_message("invalid json", &tx, &metrics, &mut last_seq).await;

        assert_eq!(metrics.parse_errors.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.messages_received.load(Ordering::Relaxed), 0);
        assert_eq!(last_seq, None);
    }

    #[tokio::test]
    async fn test_process_text_message_confirmed() {
        let (tx, mut rx) = mpsc::channel(16);
        let metrics = std::sync::Arc::new(FeedMetrics::new());
        let mut last_seq = None;

        let json = r#"{
            "version": 1,
            "messages": [],
            "confirmedSequenceNumberMessage": {"sequenceNumber": 99}
        }"#;

        process_text_message(json, &tx, &metrics, &mut last_seq).await;

        let event = rx.try_recv().expect("should have confirmed event");
        match event {
            FeedEvent::Confirmed {
                sequence_number, ..
            } => {
                assert_eq!(sequence_number, 99);
            }
            _ => panic!("expected Confirmed event"),
        }
    }

    #[test]
    fn test_reconnect_backoff() {
        let mut backoff = ReconnectBackoff::new();
        assert_eq!(backoff.next_delay(), Duration::from_millis(100));
        assert_eq!(backoff.next_delay(), Duration::from_millis(200));
        assert_eq!(backoff.next_delay(), Duration::from_millis(400));
        assert_eq!(backoff.next_delay(), Duration::from_millis(800));
        assert_eq!(backoff.next_delay(), Duration::from_millis(1600));
        assert_eq!(backoff.next_delay(), Duration::from_millis(3200));
        assert_eq!(backoff.next_delay(), Duration::from_millis(6400));
        // Should cap at 10s
        assert_eq!(backoff.next_delay(), Duration::from_secs(10));
        assert_eq!(backoff.next_delay(), Duration::from_secs(10));

        backoff.reset();
        assert_eq!(backoff.next_delay(), Duration::from_millis(100));
    }

    #[test]
    fn test_feed_metrics_default() {
        let metrics = FeedMetrics::new();
        assert_eq!(metrics.messages_received.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.parse_errors.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.reconnections.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.sequence_gaps.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_consumer_construction() {
        let config = FeedConfig {
            url: "ws://localhost:9642".into(),
            channel_buffer: default_channel_buffer(),
            ping_interval_ms: default_ping_interval_ms(),
            pong_timeout_ms: default_pong_timeout_ms(),
            stall_threshold_ms: default_stall_threshold_ms(),
        };
        let consumer = NitroFeedConsumer::new(config);
        let debug = format!("{consumer:?}");
        assert!(debug.contains("localhost:9642"));
    }

    #[tokio::test]
    async fn test_process_consecutive_messages_no_gap() {
        let (tx, mut rx) = mpsc::channel(16);
        let metrics = std::sync::Arc::new(FeedMetrics::new());
        let mut last_seq = None;

        // First message at seq 10
        let json1 =
            r#"{"version":1,"messages":[{"sequenceNumber":10,"message":null,"signature":null}]}"#;
        process_text_message(json1, &tx, &metrics, &mut last_seq).await;

        // Second message at seq 11 — no gap
        let json2 =
            r#"{"version":1,"messages":[{"sequenceNumber":11,"message":null,"signature":null}]}"#;
        process_text_message(json2, &tx, &metrics, &mut last_seq).await;

        assert_eq!(metrics.sequence_gaps.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.messages_received.load(Ordering::Relaxed), 2);
        assert_eq!(last_seq, Some(11));

        // Drain events — both should be Message events
        let e1 = rx.try_recv().expect("event 1");
        let e2 = rx.try_recv().expect("event 2");
        assert!(matches!(
            e1,
            FeedEvent::Message {
                sequence_number: 10,
                ..
            }
        ));
        assert!(matches!(
            e2,
            FeedEvent::Message {
                sequence_number: 11,
                ..
            }
        ));
    }
}
