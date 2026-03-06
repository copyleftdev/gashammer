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

//! Nitro sequencer container definition.
//!
//! Runs the Nitro node in sequencer mode with embedded batch poster
//! and `--chain.dev-init` for automatic L2 genesis.
//! See RFC-0013 §2.2.

use testcontainers::core::{ContainerPort, WaitFor};
use testcontainers::Image;

/// Default Nitro node Docker image.
pub const NITRO_IMAGE: &str = "offchainlabs/nitro-node";

/// Default Nitro node image tag.
pub const NITRO_TAG: &str = "latest";

/// Sequencer RPC port inside the container.
pub const SEQUENCER_RPC_PORT: u16 = 8547;

/// Sequencer WebSocket port inside the container.
pub const SEQUENCER_WS_PORT: u16 = 8548;

/// Nitro sequencer container.
///
/// Runs the Nitro node as a sequencer with:
/// - Embedded batch poster
/// - Dev-init chain configuration
/// - HTTP RPC on port 8547
/// - Feed output for the relay
#[derive(Debug, Clone)]
pub struct NitroSequencerImage {
    tag: String,
    l1_rpc_url: String,
}

impl NitroSequencerImage {
    /// Create a sequencer image pointing at the given L1 RPC URL.
    pub fn new(l1_rpc_url: impl Into<String>) -> Self {
        Self {
            tag: NITRO_TAG.to_string(),
            l1_rpc_url: l1_rpc_url.into(),
        }
    }

    /// Use a specific image tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = tag.into();
        self
    }
}

impl Image for NitroSequencerImage {
    fn name(&self) -> &str {
        NITRO_IMAGE
    }

    fn tag(&self) -> &str {
        &self.tag
    }

    fn ready_conditions(&self) -> Vec<WaitFor> {
        vec![WaitFor::message_on_stdout("HTTP server started")]
    }

    fn expose_ports(&self) -> &[ContainerPort] {
        &[
            ContainerPort::Tcp(SEQUENCER_RPC_PORT),
            ContainerPort::Tcp(SEQUENCER_WS_PORT),
        ]
    }

    fn cmd(&self) -> impl IntoIterator<Item = impl Into<std::borrow::Cow<'_, str>>> {
        vec![
            "--node.sequencer".to_string(),
            "--node.dangerous.no-l1-listener".to_string(),
            "--chain.dev-init".to_string(),
            format!("--parent-chain.connection.url={}", self.l1_rpc_url),
            "--http.addr=0.0.0.0".to_string(),
            format!("--http.port={SEQUENCER_RPC_PORT}"),
            "--http.api=eth,net,web3,debug,arb,arbdebug".to_string(),
            "--http.corsdomain=*".to_string(),
            "--http.vhosts=*".to_string(),
            "--node.feed.output.enable".to_string(),
            "--node.feed.output.addr=0.0.0.0".to_string(),
            "--node.batch-poster.enable".to_string(),
            "--node.batch-poster.parent-chain-wallet.only-create-key".to_string(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sequencer_image() {
        let image = NitroSequencerImage::new("http://l1:8545");
        assert_eq!(image.name(), NITRO_IMAGE);
        assert_eq!(image.tag(), NITRO_TAG);
    }

    #[test]
    fn test_custom_tag() {
        let image = NitroSequencerImage::new("http://l1:8545").with_tag("v3.0.0");
        assert_eq!(image.tag(), "v3.0.0");
    }

    #[test]
    fn test_exposes_ports() {
        let image = NitroSequencerImage::new("http://l1:8545");
        let ports = image.expose_ports();
        assert!(ports.contains(&ContainerPort::Tcp(SEQUENCER_RPC_PORT)));
        assert!(ports.contains(&ContainerPort::Tcp(SEQUENCER_WS_PORT)));
    }

    #[test]
    fn test_cmd_includes_sequencer_mode() {
        let image = NitroSequencerImage::new("http://l1:8545");
        let cmd: Vec<String> = image
            .cmd()
            .into_iter()
            .map(|s| s.into().into_owned())
            .collect();
        assert!(cmd.contains(&"--node.sequencer".to_string()));
        assert!(cmd.contains(&"--chain.dev-init".to_string()));
    }

    #[test]
    fn test_cmd_includes_l1_url() {
        let image = NitroSequencerImage::new("http://l1:8545");
        let cmd: Vec<String> = image
            .cmd()
            .into_iter()
            .map(|s| s.into().into_owned())
            .collect();
        assert!(cmd.iter().any(|s| s.contains("http://l1:8545")));
    }
}
