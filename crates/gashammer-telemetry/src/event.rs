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

//! Telemetry event model — typed events for all pipeline stages.
//!
//! Every measurable action in GasHammer emits a `TelemetryEvent` with
//! a monotonic timestamp, wall clock, and typed payload.
//!
//! Ref: RFC-0006 §2.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Event envelope
// ---------------------------------------------------------------------------

/// A telemetry event envelope carrying a typed payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEvent {
    /// Unique event identifier.
    pub event_id: Uuid,
    /// Edge that produced this event.
    pub edge_id: Uuid,
    /// Run this event belongs to.
    pub run_id: Uuid,
    /// Phase index within the run (0-based).
    pub phase_index: u32,
    /// Monotonic nanoseconds since edge start (for ordering).
    pub monotonic_ns: u64,
    /// Wall-clock nanoseconds (Unix epoch, for correlation).
    pub wall_ns: u64,
    /// Event payload.
    pub payload: EventPayload,
}

// ---------------------------------------------------------------------------
// Event payloads
// ---------------------------------------------------------------------------

/// Typed event payloads covering all telemetry categories.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventPayload {
    // -- Transaction lifecycle --
    /// Transaction submitted to the sequencer.
    TxSubmitted {
        tx_hash: String,
        gas_limit: u64,
        template_name: String,
        account: String,
        nonce: u64,
    },
    /// Transaction accepted into sequencer feed.
    TxAccepted {
        tx_hash: String,
        acceptance_latency_ms: u64,
    },
    /// Transaction included in a block.
    TxIncluded {
        tx_hash: String,
        block_number: u64,
        gas_used: u64,
        inclusion_latency_ms: u64,
    },
    /// Transaction failed (revert, out of gas, etc.).
    TxFailed {
        tx_hash: String,
        reason: String,
        gas_used: u64,
    },
    /// Transaction timed out — never confirmed.
    TxTimeout { tx_hash: String, waited_ms: u64 },

    // -- Feed events --
    /// Connected to sequencer feed.
    FeedConnected { endpoint: String },
    /// Disconnected from sequencer feed.
    FeedDisconnected { reason: String },
    /// Gap detected in feed sequence numbers.
    FeedGap {
        expected_seq: u64,
        received_seq: u64,
    },
    /// Feed stall — no messages for too long.
    FeedStall { silence_ms: u64 },

    // -- Fault injection --
    /// Fault injected.
    FaultInjected {
        fault_type: String,
        description: String,
    },
    /// Fault cleared/reverted.
    FaultCleared { fault_type: String },

    // -- Run lifecycle --
    /// Phase started.
    PhaseStarted {
        phase_index: u32,
        phase_name: String,
    },
    /// Barrier released — all edges synchronized.
    BarrierReleased { barrier_id: String },
    /// Edge is overloaded — can't keep up with target rate.
    EdgeOverloaded {
        target_gas_per_sec: u64,
        actual_gas_per_sec: u64,
        buffer_utilization: f64,
    },
}

impl TelemetryEvent {
    /// Create a new event with a generated UUID.
    pub fn new(
        edge_id: Uuid,
        run_id: Uuid,
        phase_index: u32,
        monotonic_ns: u64,
        wall_ns: u64,
        payload: EventPayload,
    ) -> Self {
        Self {
            event_id: Uuid::new_v4(),
            edge_id,
            run_id,
            phase_index,
            monotonic_ns,
            wall_ns,
            payload,
        }
    }

    /// Returns the event type name (for metrics/routing).
    pub fn event_type(&self) -> &'static str {
        match &self.payload {
            EventPayload::TxSubmitted { .. } => "tx_submitted",
            EventPayload::TxAccepted { .. } => "tx_accepted",
            EventPayload::TxIncluded { .. } => "tx_included",
            EventPayload::TxFailed { .. } => "tx_failed",
            EventPayload::TxTimeout { .. } => "tx_timeout",
            EventPayload::FeedConnected { .. } => "feed_connected",
            EventPayload::FeedDisconnected { .. } => "feed_disconnected",
            EventPayload::FeedGap { .. } => "feed_gap",
            EventPayload::FeedStall { .. } => "feed_stall",
            EventPayload::FaultInjected { .. } => "fault_injected",
            EventPayload::FaultCleared { .. } => "fault_cleared",
            EventPayload::PhaseStarted { .. } => "phase_started",
            EventPayload::BarrierReleased { .. } => "barrier_released",
            EventPayload::EdgeOverloaded { .. } => "edge_overloaded",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_event(payload: EventPayload) -> TelemetryEvent {
        TelemetryEvent::new(
            Uuid::nil(),
            Uuid::nil(),
            0,
            1_000_000,
            1_700_000_000_000_000_000,
            payload,
        )
    }

    #[test]
    fn test_event_serialization_roundtrip() {
        let payloads = vec![
            EventPayload::TxSubmitted {
                tx_hash: "0xabc".into(),
                gas_limit: 21_000,
                template_name: "simple-transfer".into(),
                account: "0x123".into(),
                nonce: 0,
            },
            EventPayload::TxAccepted {
                tx_hash: "0xabc".into(),
                acceptance_latency_ms: 42,
            },
            EventPayload::TxIncluded {
                tx_hash: "0xabc".into(),
                block_number: 100,
                gas_used: 21_000,
                inclusion_latency_ms: 250,
            },
            EventPayload::TxFailed {
                tx_hash: "0xabc".into(),
                reason: "out of gas".into(),
                gas_used: 21_000,
            },
            EventPayload::TxTimeout {
                tx_hash: "0xabc".into(),
                waited_ms: 60_000,
            },
            EventPayload::FeedConnected {
                endpoint: "ws://localhost:9642".into(),
            },
            EventPayload::FeedDisconnected {
                reason: "timeout".into(),
            },
            EventPayload::FeedGap {
                expected_seq: 100,
                received_seq: 105,
            },
            EventPayload::FeedStall { silence_ms: 5000 },
            EventPayload::FaultInjected {
                fault_type: "network_latency".into(),
                description: "100ms added".into(),
            },
            EventPayload::FaultCleared {
                fault_type: "network_latency".into(),
            },
            EventPayload::PhaseStarted {
                phase_index: 0,
                phase_name: "warmup".into(),
            },
            EventPayload::BarrierReleased {
                barrier_id: "phase-0-start".into(),
            },
            EventPayload::EdgeOverloaded {
                target_gas_per_sec: 1_000_000,
                actual_gas_per_sec: 800_000,
                buffer_utilization: 0.95,
            },
        ];

        for payload in payloads {
            let event = sample_event(payload);
            let json = serde_json::to_string(&event).expect("serialize");
            let parsed: TelemetryEvent = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(event.event_id, parsed.event_id);
            assert_eq!(event.monotonic_ns, parsed.monotonic_ns);
        }
    }

    #[test]
    fn test_event_type_names() {
        let cases = vec![
            (
                EventPayload::TxSubmitted {
                    tx_hash: "".into(),
                    gas_limit: 0,
                    template_name: "".into(),
                    account: "".into(),
                    nonce: 0,
                },
                "tx_submitted",
            ),
            (EventPayload::FeedStall { silence_ms: 0 }, "feed_stall"),
            (
                EventPayload::EdgeOverloaded {
                    target_gas_per_sec: 0,
                    actual_gas_per_sec: 0,
                    buffer_utilization: 0.0,
                },
                "edge_overloaded",
            ),
        ];

        for (payload, expected_type) in cases {
            let event = sample_event(payload);
            assert_eq!(event.event_type(), expected_type);
        }
    }

    #[test]
    fn test_event_ids_are_unique() {
        let payload = EventPayload::FeedStall { silence_ms: 0 };
        let e1 = sample_event(payload.clone());
        let e2 = sample_event(payload);
        assert_ne!(e1.event_id, e2.event_id);
    }
}
