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

//! Iptables adapter — connection reset and port block faults.
//!
//! Wraps Linux `iptables` commands to inject firewall-level faults.
//! Requires CAP_NET_ADMIN or root.
//!
//! Ref: RFC-0008 §3.2.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::process::Command;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::adapter::{FaultAdapter, FaultHandle, FaultSpec, FaultType, PreflightResult};

/// An active iptables rule tracked for cleanup.
#[derive(Debug, Clone)]
struct ActiveIptablesRule {
    /// The iptables args used for injection (for cleanup via -D).
    rule_args: Vec<String>,
}

/// Iptables adapter for connection-level faults.
///
/// Supports: ConnectionReset, PortBlock.
///
/// Parameters:
/// - `port`: target port (PortBlock, default: none)
/// - `protocol`: tcp or udp (default: "tcp")
/// - Target field in FaultSpec is used as the source IP
pub struct IptablesAdapter {
    active: Arc<RwLock<HashMap<Uuid, ActiveIptablesRule>>>,
}

impl IptablesAdapter {
    pub fn new() -> Self {
        Self {
            active: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn build_rule_args(spec: &FaultSpec) -> Result<Vec<String>, String> {
        let protocol = spec
            .parameters
            .get("protocol")
            .cloned()
            .unwrap_or_else(|| "tcp".into());

        match spec.fault_type {
            FaultType::ConnectionReset => {
                let mut args = vec![
                    "INPUT".into(),
                    "-p".into(),
                    protocol,
                    "-s".into(),
                    spec.target.clone(),
                ];
                if let Some(port) = spec.parameters.get("port") {
                    args.extend_from_slice(&["--dport".into(), port.clone()]);
                }
                args.extend_from_slice(&[
                    "-j".into(),
                    "REJECT".into(),
                    "--reject-with".into(),
                    "tcp-reset".into(),
                ]);
                Ok(args)
            }
            FaultType::PortBlock => {
                let port = spec
                    .parameters
                    .get("port")
                    .ok_or("missing parameter: port")?;
                let args = vec![
                    "INPUT".into(),
                    "-p".into(),
                    protocol,
                    "--dport".into(),
                    port.clone(),
                    "-j".into(),
                    "DROP".into(),
                ];
                Ok(args)
            }
            _ => Err(format!("unsupported fault type: {:?}", spec.fault_type)),
        }
    }

    async fn run_iptables(action: &str, rule_args: &[String]) -> Result<(), String> {
        let output = Command::new("iptables")
            .arg(action)
            .args(rule_args)
            .output()
            .await
            .map_err(|e| format!("failed to execute iptables: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("iptables failed: {stderr}"));
        }
        Ok(())
    }
}

impl Default for IptablesAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl FaultAdapter for IptablesAdapter {
    fn name(&self) -> &str {
        "iptables"
    }

    fn supported_faults(&self) -> Vec<FaultType> {
        vec![FaultType::ConnectionReset, FaultType::PortBlock]
    }

    fn preflight_check(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = PreflightResult> + Send + '_>> {
        Box::pin(async {
            let mut issues = Vec::new();

            match Command::new("iptables").arg("--version").output().await {
                Ok(output) if output.status.success() => {}
                Ok(_) => issues.push("iptables binary found but returned error".into()),
                Err(_) => issues.push("iptables binary not found".into()),
            }

            PreflightResult {
                adapter_name: "iptables".into(),
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
            let rule_args = Self::build_rule_args(&spec)?;

            Self::run_iptables("-A", &rule_args).await?;

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
                "iptables: fault injected"
            );

            self.active
                .write()
                .await
                .insert(handle.id, ActiveIptablesRule { rule_args });
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
                    .ok_or_else(|| format!("iptables fault {handle_id} not found"))?
            };

            Self::run_iptables("-D", &rule.rule_args).await?;
            tracing::info!(fault_id = %handle_id, "iptables: fault cleared");
            Ok(())
        })
    }

    fn clear_all(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u32, String>> + Send + '_>> {
        Box::pin(async {
            let rules: Vec<(Uuid, ActiveIptablesRule)> = {
                let mut active = self.active.write().await;
                active.drain().collect()
            };

            let mut cleared = 0u32;
            for (id, rule) in &rules {
                if Self::run_iptables("-D", &rule.rule_args).await.is_ok() {
                    cleared += 1;
                    tracing::info!(fault_id = %id, "iptables: fault cleared (clear_all)");
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
    fn test_iptables_supported_faults() {
        let adapter = IptablesAdapter::new();
        let faults = adapter.supported_faults();
        assert!(faults.contains(&FaultType::ConnectionReset));
        assert!(faults.contains(&FaultType::PortBlock));
        assert!(!faults.contains(&FaultType::NetworkLatency));
    }

    #[test]
    fn test_iptables_name() {
        let adapter = IptablesAdapter::new();
        assert_eq!(adapter.name(), "iptables");
    }

    #[test]
    fn test_build_connection_reset_args() {
        let spec = FaultSpec {
            fault_type: FaultType::ConnectionReset,
            target: "10.0.0.1".into(),
            parameters: HashMap::new(),
            duration: None,
        };

        let args = IptablesAdapter::build_rule_args(&spec).unwrap();
        assert!(args.contains(&"INPUT".to_string()));
        assert!(args.contains(&"-p".to_string()));
        assert!(args.contains(&"tcp".to_string()));
        assert!(args.contains(&"10.0.0.1".to_string()));
        assert!(args.contains(&"REJECT".to_string()));
        assert!(args.contains(&"tcp-reset".to_string()));
    }

    #[test]
    fn test_build_port_block_args() {
        let spec = FaultSpec {
            fault_type: FaultType::PortBlock,
            target: "0.0.0.0".into(),
            parameters: {
                let mut p = HashMap::new();
                p.insert("port".into(), "8547".into());
                p
            },
            duration: None,
        };

        let args = IptablesAdapter::build_rule_args(&spec).unwrap();
        assert!(args.contains(&"8547".to_string()));
        assert!(args.contains(&"DROP".to_string()));
    }

    #[test]
    fn test_build_port_block_missing_port_fails() {
        let spec = FaultSpec {
            fault_type: FaultType::PortBlock,
            target: "0.0.0.0".into(),
            parameters: HashMap::new(),
            duration: None,
        };

        let result = IptablesAdapter::build_rule_args(&spec);
        assert!(result.is_err());
    }

    #[test]
    fn test_build_connection_reset_with_port() {
        let spec = FaultSpec {
            fault_type: FaultType::ConnectionReset,
            target: "10.0.0.1".into(),
            parameters: {
                let mut p = HashMap::new();
                p.insert("port".into(), "443".into());
                p
            },
            duration: None,
        };

        let args = IptablesAdapter::build_rule_args(&spec).unwrap();
        assert!(args.contains(&"--dport".to_string()));
        assert!(args.contains(&"443".to_string()));
    }

    #[tokio::test]
    async fn test_iptables_preflight_check() {
        let adapter = IptablesAdapter::new();
        let result = adapter.preflight_check().await;
        assert_eq!(result.adapter_name, "iptables");
        // iptables may or may not be available in test env
    }
}
