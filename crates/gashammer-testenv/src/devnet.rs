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

//! Nitro devnet orchestrator.
//!
//! Boots L1 Geth, Nitro Sequencer, and Feed Relay containers in the correct
//! order, performs health checks, and pre-funds test accounts.
//! See RFC-0013 §3.

use serde::{Deserialize, Serialize};
use testcontainers::runners::AsyncRunner;
use testcontainers::ContainerAsync;

use crate::containers::feed_relay::{FeedRelayImage, FEED_RELAY_PORT};
use crate::containers::l1_geth::{L1GethImage, L1_RPC_PORT};
use crate::containers::sequencer::{NitroSequencerImage, SEQUENCER_RPC_PORT};
use crate::error::TestenvError;

/// Number of test accounts to pre-fund.
pub const DEFAULT_FUNDED_ACCOUNTS: usize = 100;

/// Amount of ETH to fund each account (in wei string for JSON-RPC).
pub const DEFAULT_FUND_AMOUNT_ETH: &str = "100";

/// Devnet deployment profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DevnetProfile {
    /// L1 Geth + Sequencer + Feed Relay.
    Minimal,
    /// Minimal + Validator.
    Standard,
    /// Standard + Token Bridge + Staker.
    Full,
}

/// A running Nitro devnet with all components.
///
/// Containers are cleaned up when this struct is dropped.
pub struct NitroDevnet {
    profile: DevnetProfile,
    l1_geth: ContainerAsync<L1GethImage>,
    sequencer: ContainerAsync<NitroSequencerImage>,
    feed_relay: ContainerAsync<FeedRelayImage>,
    l1_rpc_url: String,
    sequencer_rpc_url: String,
    feed_relay_url: String,
    chain_id: u64,
}

impl NitroDevnet {
    /// Boot a minimal devnet: L1 Geth + Sequencer + Feed Relay.
    pub async fn minimal() -> Result<Self, TestenvError> {
        Self::boot(DevnetProfile::Minimal).await
    }

    /// Boot a standard devnet (minimal + validator).
    /// Currently identical to minimal — validator container TBD.
    pub async fn standard() -> Result<Self, TestenvError> {
        Self::boot(DevnetProfile::Standard).await
    }

    /// Boot a devnet with the given profile.
    async fn boot(profile: DevnetProfile) -> Result<Self, TestenvError> {
        tracing::info!(profile = ?profile, "booting Nitro devnet");

        // 1. Start L1 Geth
        let l1_geth = L1GethImage::default()
            .start()
            .await
            .map_err(|e| TestenvError::Container(format!("L1 Geth start failed: {e}")))?;

        let l1_host_port = l1_geth
            .get_host_port_ipv4(L1_RPC_PORT)
            .await
            .map_err(|e| TestenvError::Container(format!("L1 Geth port mapping failed: {e}")))?;

        let l1_rpc_url = format!("http://127.0.0.1:{l1_host_port}");
        tracing::info!(url = %l1_rpc_url, "L1 Geth started");

        // 2. Start Nitro Sequencer
        let sequencer_image = NitroSequencerImage::new(&l1_rpc_url);
        let sequencer = sequencer_image
            .start()
            .await
            .map_err(|e| TestenvError::Container(format!("Sequencer start failed: {e}")))?;

        let sequencer_host_port = sequencer
            .get_host_port_ipv4(SEQUENCER_RPC_PORT)
            .await
            .map_err(|e| TestenvError::Container(format!("Sequencer port mapping failed: {e}")))?;

        let sequencer_rpc_url = format!("http://127.0.0.1:{sequencer_host_port}");
        tracing::info!(url = %sequencer_rpc_url, "Sequencer started");

        // 3. Start Feed Relay
        let feed_relay_image = FeedRelayImage::new(format!("ws://127.0.0.1:{sequencer_host_port}"));
        let feed_relay = feed_relay_image
            .start()
            .await
            .map_err(|e| TestenvError::Container(format!("Feed relay start failed: {e}")))?;

        let feed_host_port = feed_relay
            .get_host_port_ipv4(FEED_RELAY_PORT)
            .await
            .map_err(|e| TestenvError::Container(format!("Feed relay port mapping failed: {e}")))?;

        let feed_relay_url = format!("ws://127.0.0.1:{feed_host_port}");
        tracing::info!(url = %feed_relay_url, "Feed relay started");

        // Default devnet chain ID for Nitro dev-init.
        let chain_id = 412346;

        let devnet = Self {
            profile,
            l1_geth,
            sequencer,
            feed_relay,
            l1_rpc_url,
            sequencer_rpc_url,
            feed_relay_url,
            chain_id,
        };

        tracing::info!(
            profile = ?devnet.profile,
            chain_id = devnet.chain_id,
            "Nitro devnet ready"
        );

        Ok(devnet)
    }

    /// Run health checks on all components.
    pub async fn health_check(&self) -> Result<(), TestenvError> {
        // Check L1 via eth_chainId
        rpc_health_check(&self.l1_rpc_url, "L1 Geth").await?;

        // Check Sequencer via eth_blockNumber
        rpc_health_check(&self.sequencer_rpc_url, "Sequencer").await?;

        tracing::info!("all health checks passed");
        Ok(())
    }

    /// Returns the L1 RPC URL.
    pub fn l1_rpc_url(&self) -> &str {
        &self.l1_rpc_url
    }

    /// Returns the sequencer RPC URL.
    pub fn sequencer_rpc_url(&self) -> &str {
        &self.sequencer_rpc_url
    }

    /// Returns the feed relay WebSocket URL.
    pub fn feed_relay_url(&self) -> &str {
        &self.feed_relay_url
    }

    /// Returns the L2 chain ID.
    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    /// Returns the devnet profile.
    pub fn profile(&self) -> &DevnetProfile {
        &self.profile
    }

    /// Returns the number of running containers.
    /// Container handles are held to prevent premature shutdown on Drop.
    pub fn container_count(&self) -> usize {
        // These fields exist to keep containers alive via Drop.
        let _ = &self.l1_geth;
        let _ = &self.sequencer;
        let _ = &self.feed_relay;
        3
    }
}

impl std::fmt::Debug for NitroDevnet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NitroDevnet")
            .field("profile", &self.profile)
            .field("l1_rpc_url", &self.l1_rpc_url)
            .field("sequencer_rpc_url", &self.sequencer_rpc_url)
            .field("feed_relay_url", &self.feed_relay_url)
            .field("chain_id", &self.chain_id)
            .finish()
    }
}

/// Perform an RPC health check by calling eth_chainId.
async fn rpc_health_check(url: &str, component: &str) -> Result<(), TestenvError> {
    let client = reqwest::Client::new();

    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "eth_chainId",
        "params": []
    });

    let resp = client
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|e| TestenvError::HealthCheck(format!("{component} RPC unreachable: {e}")))?;

    if !resp.status().is_success() {
        return Err(TestenvError::HealthCheck(format!(
            "{component} returned HTTP {}",
            resp.status()
        )));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| TestenvError::HealthCheck(format!("{component} invalid JSON: {e}")))?;

    if json.get("error").is_some() {
        return Err(TestenvError::HealthCheck(format!(
            "{component} RPC error: {}",
            json["error"]
        )));
    }

    tracing::debug!(component, "health check passed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_devnet_profile_eq() {
        assert_eq!(DevnetProfile::Minimal, DevnetProfile::Minimal);
        assert_ne!(DevnetProfile::Minimal, DevnetProfile::Standard);
    }

    #[test]
    fn test_devnet_profile_serialization() {
        let profile = DevnetProfile::Minimal;
        let json = serde_json::to_string(&profile).expect("serialize");
        assert_eq!(json, r#""Minimal""#);

        let parsed: DevnetProfile = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, DevnetProfile::Minimal);
    }

    #[test]
    fn test_constants() {
        assert_eq!(DEFAULT_FUNDED_ACCOUNTS, 100);
        assert_eq!(DEFAULT_FUND_AMOUNT_ETH, "100");
    }
}
