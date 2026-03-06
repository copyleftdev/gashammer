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

/// Generated protobuf types and gRPC service definitions.
///
/// Package name `gashammer.v1` is part of DNA provenance (RFC-0011 §4.1).
/// All generated client code carries this package name.
#[allow(clippy::doc_markdown)]
pub mod gashammer {
    pub mod v1 {
        tonic::include_proto!("gashammer.v1");
    }
}

#[cfg(test)]
mod tests {
    use super::gashammer::v1::*;

    #[test]
    fn test_register_edge_request_default() {
        let req = RegisterEdgeRequest::default();
        assert!(req.bootstrap_token.is_empty());
        assert!(req.region.is_empty());
    }

    #[test]
    fn test_control_message_oneof() {
        let msg = ControlMessage {
            message: Some(control_message::Message::AbortRun(AbortRun {
                run_id: "test-run".into(),
                reason: "test abort".into(),
            })),
        };
        assert!(msg.message.is_some());
    }

    #[test]
    fn test_telemetry_event_oneof() {
        let event = TelemetryEvent {
            edge_id: "edge-1".into(),
            run_id: "run-1".into(),
            monotonic_ns: 1000,
            wall_ns: 2000,
            event: Some(telemetry_event::Event::TxSubmitted(TxSubmitted {
                tx_hash: vec![0xde, 0xad],
                nonce: 1,
                gas_limit: 21000,
                template_name: "simple_transfer".into(),
            })),
        };
        assert_eq!(event.edge_id, "edge-1");
    }

    #[test]
    fn test_heartbeat_message() {
        let hb = HeartbeatMessage {
            edge_id: "edge-1".into(),
            timestamp_ns: 1234567890,
            cpu_utilization: 0.75,
            memory_utilization: 0.5,
            active_connections: 3,
            current_run_id: String::new(),
        };
        assert_eq!(hb.cpu_utilization, 0.75);
    }

    #[test]
    fn test_edge_status_enum() {
        assert_eq!(EdgeStatus::Unspecified as i32, 0);
        assert_eq!(EdgeStatus::Running as i32, 3);
        assert_eq!(EdgeStatus::Completed as i32, 5);
    }
}
