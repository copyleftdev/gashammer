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

//! Edge runtime configuration.
//!
//! Supports layered config: TOML file → env vars (GASHAMMER_*) → CLI flags.
//! Ref: RFC-0004 §3, RFC-0002 §9.3.

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::EdgeError;

/// Complete edge configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeConfig {
    /// Region identifier (e.g., "us-west-2").
    pub region: String,

    /// Hive gRPC endpoint (e.g., "http://hive:9090").
    pub hive_endpoint: String,

    /// Bootstrap token for hive registration.
    pub bootstrap_token: String,

    /// Sequencer RPC configuration.
    pub sequencer: RpcEndpointConfig,

    /// Gateway RPC configuration (confirmed state, historical queries).
    pub gateway: Option<RpcEndpointConfig>,

    /// Feed relay WebSocket URL.
    pub feed_relay_url: Option<String>,

    /// Heartbeat interval to hive (default: 5s).
    #[serde(default = "default_heartbeat_interval_ms")]
    pub heartbeat_interval_ms: u64,

    /// Graceful shutdown timeout (default: 30s).
    #[serde(default = "default_shutdown_timeout_ms")]
    pub shutdown_timeout_ms: u64,

    /// Supported adapters (e.g., ["nitro-rpc", "fault-netem"]).
    #[serde(default)]
    pub supported_adapters: Vec<String>,

    /// Instance profile (hardware description).
    #[serde(default)]
    pub instance_profile: InstanceProfileConfig,
}

/// RPC endpoint configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcEndpointConfig {
    pub url: String,
    #[serde(default = "default_rpc_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_rpc_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_rpc_retry_delay_ms")]
    pub retry_base_delay_ms: u64,
}

/// Instance hardware profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceProfileConfig {
    #[serde(default = "default_cpu_cores")]
    pub cpu_cores: u32,
    #[serde(default = "default_memory_mb")]
    pub memory_mb: u64,
    #[serde(default = "default_network_class")]
    pub network_class: String,
}

impl Default for InstanceProfileConfig {
    fn default() -> Self {
        Self {
            cpu_cores: default_cpu_cores(),
            memory_mb: default_memory_mb(),
            network_class: default_network_class(),
        }
    }
}

// Defaults
fn default_heartbeat_interval_ms() -> u64 {
    5_000
}
fn default_shutdown_timeout_ms() -> u64 {
    30_000
}
fn default_rpc_timeout_ms() -> u64 {
    10_000
}
fn default_rpc_max_retries() -> u32 {
    3
}
fn default_rpc_retry_delay_ms() -> u64 {
    500
}
fn default_cpu_cores() -> u32 {
    4
}
fn default_memory_mb() -> u64 {
    8192
}
fn default_network_class() -> String {
    "standard".into()
}

impl EdgeConfig {
    /// Load configuration from a TOML file.
    pub fn from_file(path: &Path) -> Result<Self, EdgeError> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            EdgeError::Config(format!(
                "failed to read config file {}: {e}",
                path.display()
            ))
        })?;
        Self::from_toml(&content)
    }

    /// Parse configuration from a TOML string.
    pub fn from_toml(content: &str) -> Result<Self, EdgeError> {
        toml::from_str(content).map_err(|e| EdgeError::Config(format!("invalid TOML config: {e}")))
    }

    /// Returns the heartbeat interval as a Duration.
    pub fn heartbeat_interval(&self) -> Duration {
        Duration::from_millis(self.heartbeat_interval_ms)
    }

    /// Returns the shutdown timeout as a Duration.
    pub fn shutdown_timeout(&self) -> Duration {
        Duration::from_millis(self.shutdown_timeout_ms)
    }

    /// Convert sequencer config to a NitroRpcProvider-compatible RpcConfig.
    pub fn sequencer_rpc_config(&self) -> gashammer_nitro::rpc::RpcConfig {
        gashammer_nitro::rpc::RpcConfig {
            url: self.sequencer.url.clone(),
            timeout_ms: self.sequencer.timeout_ms,
            max_retries: self.sequencer.max_retries,
            retry_base_delay_ms: self.sequencer.retry_base_delay_ms,
        }
    }

    /// Convert gateway config to RpcConfig, if present.
    pub fn gateway_rpc_config(&self) -> Option<gashammer_nitro::rpc::RpcConfig> {
        self.gateway
            .as_ref()
            .map(|gw| gashammer_nitro::rpc::RpcConfig {
                url: gw.url.clone(),
                timeout_ms: gw.timeout_ms,
                max_retries: gw.max_retries,
                retry_base_delay_ms: gw.retry_base_delay_ms,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL_TOML: &str = r#"
region = "us-west-2"
hive_endpoint = "http://localhost:9090"
bootstrap_token = "test-token"

[sequencer]
url = "http://localhost:8547"
"#;

    const FULL_TOML: &str = r#"
region = "eu-west-1"
hive_endpoint = "https://hive.example.com:9090"
bootstrap_token = "prod-token-abc"
feed_relay_url = "ws://localhost:9642"
heartbeat_interval_ms = 3000
shutdown_timeout_ms = 60000
supported_adapters = ["nitro-rpc", "fault-netem"]

[sequencer]
url = "http://sequencer:8547"
timeout_ms = 15000
max_retries = 5
retry_base_delay_ms = 1000

[gateway]
url = "http://gateway:8547"
timeout_ms = 20000
max_retries = 3
retry_base_delay_ms = 500

[instance_profile]
cpu_cores = 16
memory_mb = 32768
network_class = "enhanced"
"#;

    #[test]
    fn test_minimal_config_parsing() {
        let config = EdgeConfig::from_toml(MINIMAL_TOML).expect("should parse minimal config");
        assert_eq!(config.region, "us-west-2");
        assert_eq!(config.hive_endpoint, "http://localhost:9090");
        assert_eq!(config.sequencer.url, "http://localhost:8547");
        // Defaults applied
        assert_eq!(config.heartbeat_interval_ms, 5_000);
        assert_eq!(config.shutdown_timeout_ms, 30_000);
        assert_eq!(config.sequencer.timeout_ms, 10_000);
        assert!(config.gateway.is_none());
        assert!(config.feed_relay_url.is_none());
    }

    #[test]
    fn test_full_config_parsing() {
        let config = EdgeConfig::from_toml(FULL_TOML).expect("should parse full config");
        assert_eq!(config.region, "eu-west-1");
        assert_eq!(config.heartbeat_interval_ms, 3_000);
        assert_eq!(config.shutdown_timeout_ms, 60_000);
        assert_eq!(config.supported_adapters.len(), 2);
        assert!(config.gateway.is_some());
        assert_eq!(
            config.feed_relay_url.as_deref(),
            Some("ws://localhost:9642")
        );

        let gw = config.gateway.unwrap();
        assert_eq!(gw.url, "http://gateway:8547");
        assert_eq!(gw.timeout_ms, 20_000);

        assert_eq!(config.instance_profile.cpu_cores, 16);
        assert_eq!(config.instance_profile.memory_mb, 32_768);
    }

    #[test]
    fn test_heartbeat_interval_duration() {
        let config = EdgeConfig::from_toml(MINIMAL_TOML).unwrap();
        assert_eq!(config.heartbeat_interval(), Duration::from_secs(5));
    }

    #[test]
    fn test_shutdown_timeout_duration() {
        let config = EdgeConfig::from_toml(MINIMAL_TOML).unwrap();
        assert_eq!(config.shutdown_timeout(), Duration::from_secs(30));
    }

    #[test]
    fn test_sequencer_rpc_config_conversion() {
        let config = EdgeConfig::from_toml(MINIMAL_TOML).unwrap();
        let rpc = config.sequencer_rpc_config();
        assert_eq!(rpc.url, "http://localhost:8547");
        assert_eq!(rpc.timeout_ms, 10_000);
    }

    #[test]
    fn test_gateway_rpc_config_when_absent() {
        let config = EdgeConfig::from_toml(MINIMAL_TOML).unwrap();
        assert!(config.gateway_rpc_config().is_none());
    }

    #[test]
    fn test_gateway_rpc_config_when_present() {
        let config = EdgeConfig::from_toml(FULL_TOML).unwrap();
        let gw = config.gateway_rpc_config().expect("should have gateway");
        assert_eq!(gw.url, "http://gateway:8547");
    }

    #[test]
    fn test_invalid_toml_returns_error() {
        let result = EdgeConfig::from_toml("not valid toml {{{}");
        assert!(result.is_err());
    }

    #[test]
    fn test_config_serialization_roundtrip() {
        let config = EdgeConfig::from_toml(FULL_TOML).unwrap();
        let toml_str = toml::to_string(&config).expect("serialize");
        let parsed = EdgeConfig::from_toml(&toml_str).expect("re-parse");
        assert_eq!(config.region, parsed.region);
        assert_eq!(config.heartbeat_interval_ms, parsed.heartbeat_interval_ms);
    }

    #[test]
    fn test_instance_profile_defaults() {
        let profile = InstanceProfileConfig::default();
        assert_eq!(profile.cpu_cores, 4);
        assert_eq!(profile.memory_mb, 8192);
        assert_eq!(profile.network_class, "standard");
    }
}
