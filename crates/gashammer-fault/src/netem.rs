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

//! Network emulation adapter — tc netem for latency, loss, jitter.
//!
//! Wraps Linux `tc qdisc` commands to inject network faults.
//! Requires CAP_NET_ADMIN or root.
//!
//! Ref: RFC-0008 §3.1.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::process::Command;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::adapter::{FaultAdapter, FaultHandle, FaultSpec, FaultType, PreflightResult};

/// Default network interface for netem rules.
const DEFAULT_IFACE: &str = "eth0";

/// Active netem rule tracked for cleanup.
#[derive(Debug, Clone)]
struct ActiveRule {
    interface: String,
}

/// Network emulation adapter using Linux `tc netem`.
///
/// Supports: NetworkLatency, PacketLoss, BandwidthLimit, NetworkJitter.
///
/// Parameters:
/// - `interface`: network interface (default: "eth0")
/// - `delay_ms`: latency in milliseconds (NetworkLatency)
/// - `jitter_ms`: jitter variation in milliseconds (NetworkJitter)
/// - `loss_pct`: packet loss percentage (PacketLoss)
/// - `rate`: bandwidth limit, e.g. "1mbit" (BandwidthLimit)
pub struct NetemAdapter {
    active: Arc<RwLock<HashMap<Uuid, ActiveRule>>>,
}

impl NetemAdapter {
    pub fn new() -> Self {
        Self {
            active: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn get_interface(params: &HashMap<String, String>) -> String {
        params
            .get("interface")
            .cloned()
            .unwrap_or_else(|| DEFAULT_IFACE.into())
    }

    fn build_netem_args(spec: &FaultSpec) -> Result<Vec<String>, String> {
        let iface = Self::get_interface(&spec.parameters);
        let mut args = vec![
            "qdisc".into(),
            "add".into(),
            "dev".into(),
            iface,
            "root".into(),
            "netem".into(),
        ];

        match spec.fault_type {
            FaultType::NetworkLatency => {
                let delay = spec
                    .parameters
                    .get("delay_ms")
                    .ok_or("missing parameter: delay_ms")?;
                args.push("delay".into());
                args.push(format!("{delay}ms"));
            }
            FaultType::NetworkJitter => {
                let delay = spec
                    .parameters
                    .get("delay_ms")
                    .ok_or("missing parameter: delay_ms")?;
                let jitter = spec
                    .parameters
                    .get("jitter_ms")
                    .ok_or("missing parameter: jitter_ms")?;
                args.push("delay".into());
                args.push(format!("{delay}ms"));
                args.push(format!("{jitter}ms"));
            }
            FaultType::PacketLoss => {
                let loss = spec
                    .parameters
                    .get("loss_pct")
                    .ok_or("missing parameter: loss_pct")?;
                args.push("loss".into());
                args.push(format!("{loss}%"));
            }
            FaultType::BandwidthLimit => {
                let rate = spec
                    .parameters
                    .get("rate")
                    .ok_or("missing parameter: rate")?;
                args.push("rate".into());
                args.push(rate.clone());
            }
            _ => return Err(format!("unsupported fault type: {:?}", spec.fault_type)),
        }

        Ok(args)
    }

    async fn run_tc(args: &[String]) -> Result<(), String> {
        let output = Command::new("tc")
            .args(args)
            .output()
            .await
            .map_err(|e| format!("failed to execute tc: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("tc failed: {stderr}"));
        }
        Ok(())
    }
}

impl Default for NetemAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl FaultAdapter for NetemAdapter {
    fn name(&self) -> &str {
        "netem"
    }

    fn supported_faults(&self) -> Vec<FaultType> {
        vec![
            FaultType::NetworkLatency,
            FaultType::PacketLoss,
            FaultType::BandwidthLimit,
            FaultType::NetworkJitter,
        ]
    }

    fn preflight_check(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = PreflightResult> + Send + '_>> {
        Box::pin(async {
            let mut issues = Vec::new();

            match Command::new("tc").arg("-Version").output().await {
                Ok(output) if output.status.success() => {}
                Ok(_) => issues.push("tc binary found but returned error".into()),
                Err(_) => issues.push("tc binary not found (install iproute2)".into()),
            }

            PreflightResult {
                adapter_name: "netem".into(),
                ready: issues.is_empty(),
                issues,
            }
        })
    }

    fn inject(
        &self,
        spec: FaultSpec,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FaultHandle, String>> + Send + '_>>
    {
        Box::pin(async move {
            let args = Self::build_netem_args(&spec)?;
            let iface = Self::get_interface(&spec.parameters);

            Self::run_tc(&args).await?;

            let handle = FaultHandle {
                id: Uuid::new_v4(),
                spec,
                injected_at_ms: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
                cleared: false,
            };

            tracing::info!(
                fault_id = %handle.id,
                fault_type = ?handle.spec.fault_type,
                %iface,
                "netem: fault injected"
            );

            self.active
                .write()
                .await
                .insert(handle.id, ActiveRule { interface: iface });
            Ok(handle)
        })
    }

    fn clear(
        &self,
        handle_id: Uuid,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>> {
        Box::pin(async move {
            let rule = {
                let mut active = self.active.write().await;
                active
                    .remove(&handle_id)
                    .ok_or_else(|| format!("netem fault {handle_id} not found"))?
            };

            let args = vec![
                "qdisc".into(),
                "del".into(),
                "dev".into(),
                rule.interface.clone(),
                "root".into(),
                "netem".into(),
            ];

            Self::run_tc(&args).await?;
            tracing::info!(fault_id = %handle_id, iface = %rule.interface, "netem: fault cleared");
            Ok(())
        })
    }

    fn clear_all(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u32, String>> + Send + '_>> {
        Box::pin(async {
            let rules: Vec<(Uuid, ActiveRule)> = {
                let mut active = self.active.write().await;
                active.drain().collect()
            };

            let mut cleared = 0u32;
            for (id, rule) in &rules {
                let args = vec![
                    "qdisc".into(),
                    "del".into(),
                    "dev".into(),
                    rule.interface.clone(),
                    "root".into(),
                    "netem".into(),
                ];
                if Self::run_tc(&args).await.is_ok() {
                    cleared += 1;
                    tracing::info!(fault_id = %id, "netem: fault cleared (clear_all)");
                }
            }
            Ok(cleared)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_netem_supported_faults() {
        let adapter = NetemAdapter::new();
        let faults = adapter.supported_faults();
        assert!(faults.contains(&FaultType::NetworkLatency));
        assert!(faults.contains(&FaultType::PacketLoss));
        assert!(faults.contains(&FaultType::BandwidthLimit));
        assert!(faults.contains(&FaultType::NetworkJitter));
        assert!(!faults.contains(&FaultType::ConnectionReset));
    }

    #[test]
    fn test_netem_name() {
        let adapter = NetemAdapter::new();
        assert_eq!(adapter.name(), "netem");
    }

    #[test]
    fn test_build_latency_args() {
        let spec = FaultSpec {
            fault_type: FaultType::NetworkLatency,
            target: "127.0.0.1".into(),
            parameters: {
                let mut p = HashMap::new();
                p.insert("delay_ms".into(), "100".into());
                p.insert("interface".into(), "lo".into());
                p
            },
            duration: None,
        };

        let args = NetemAdapter::build_netem_args(&spec).unwrap();
        assert!(args.contains(&"delay".to_string()));
        assert!(args.contains(&"100ms".to_string()));
        assert!(args.contains(&"lo".to_string()));
    }

    #[test]
    fn test_build_packet_loss_args() {
        let spec = FaultSpec {
            fault_type: FaultType::PacketLoss,
            target: "127.0.0.1".into(),
            parameters: {
                let mut p = HashMap::new();
                p.insert("loss_pct".into(), "10".into());
                p
            },
            duration: None,
        };

        let args = NetemAdapter::build_netem_args(&spec).unwrap();
        assert!(args.contains(&"loss".to_string()));
        assert!(args.contains(&"10%".to_string()));
    }

    #[test]
    fn test_build_jitter_args() {
        let spec = FaultSpec {
            fault_type: FaultType::NetworkJitter,
            target: "127.0.0.1".into(),
            parameters: {
                let mut p = HashMap::new();
                p.insert("delay_ms".into(), "50".into());
                p.insert("jitter_ms".into(), "20".into());
                p
            },
            duration: None,
        };

        let args = NetemAdapter::build_netem_args(&spec).unwrap();
        assert!(args.contains(&"delay".to_string()));
        assert!(args.contains(&"50ms".to_string()));
        assert!(args.contains(&"20ms".to_string()));
    }

    #[test]
    fn test_build_bandwidth_args() {
        let spec = FaultSpec {
            fault_type: FaultType::BandwidthLimit,
            target: "127.0.0.1".into(),
            parameters: {
                let mut p = HashMap::new();
                p.insert("rate".into(), "1mbit".into());
                p
            },
            duration: None,
        };

        let args = NetemAdapter::build_netem_args(&spec).unwrap();
        assert!(args.contains(&"rate".to_string()));
        assert!(args.contains(&"1mbit".to_string()));
    }

    #[test]
    fn test_build_missing_param_fails() {
        let spec = FaultSpec {
            fault_type: FaultType::NetworkLatency,
            target: "127.0.0.1".into(),
            parameters: HashMap::new(),
            duration: None,
        };

        let result = NetemAdapter::build_netem_args(&spec);
        assert!(result.is_err());
    }

    #[test]
    fn test_default_interface() {
        let params = HashMap::new();
        assert_eq!(NetemAdapter::get_interface(&params), DEFAULT_IFACE);
    }

    #[tokio::test]
    async fn test_netem_preflight_check() {
        let adapter = NetemAdapter::new();
        let result = adapter.preflight_check().await;
        assert_eq!(result.adapter_name, "netem");
        // tc may or may not be available in test env — just check the structure
    }
}
