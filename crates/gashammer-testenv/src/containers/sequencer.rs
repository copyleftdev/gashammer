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

//! Nitro sequencer container definition.
//!
//! Runs the Nitro node in `--dev` mode as a self-contained L2-only chain.
//! See RFC-0013 §2.2.

use testcontainers::core::{ContainerPort, WaitFor};
use testcontainers::Image;

/// Default Nitro node Docker image.
pub const NITRO_IMAGE: &str = "offchainlabs/nitro-node";

/// Default Nitro node image tag (pinned — no `latest` tag exists).
pub const NITRO_TAG: &str = "v3.9.7-75e084e";

/// Sequencer RPC port inside the container.
pub const SEQUENCER_RPC_PORT: u16 = 8547;

/// Sequencer WebSocket port inside the container.
pub const SEQUENCER_WS_PORT: u16 = 8548;

/// Nitro sequencer container.
///
/// Runs the Nitro node in `--dev` mode:
/// - Self-contained L2-only chain (no L1 required)
/// - HTTP RPC on port 8547
/// - Feed output for the relay
#[derive(Debug, Clone)]
pub struct NitroSequencerImage {
    tag: String,
}

impl NitroSequencerImage {
    /// Create a sequencer image with default settings.
    pub fn new() -> Self {
        Self {
            tag: NITRO_TAG.to_string(),
        }
    }

    /// Use a specific image tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = tag.into();
        self
    }
}

impl Default for NitroSequencerImage {
    fn default() -> Self {
        Self::new()
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
        vec![WaitFor::message_on_stderr("HTTP server started")]
    }

    fn expose_ports(&self) -> &[ContainerPort] {
        &[
            ContainerPort::Tcp(SEQUENCER_RPC_PORT),
            ContainerPort::Tcp(SEQUENCER_WS_PORT),
        ]
    }

    fn cmd(&self) -> impl IntoIterator<Item = impl Into<std::borrow::Cow<'_, str>>> {
        vec![
            "--dev".to_string(),
            "--http.addr=0.0.0.0".to_string(),
            format!("--http.port={SEQUENCER_RPC_PORT}"),
            "--http.api=eth,net,web3,debug,arb,arbdebug".to_string(),
            "--http.corsdomain=*".to_string(),
            "--http.vhosts=*".to_string(),
            "--node.feed.output.enable".to_string(),
            "--node.feed.output.addr=0.0.0.0".to_string(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sequencer_image() {
        let image = NitroSequencerImage::new();
        assert_eq!(image.name(), NITRO_IMAGE);
        assert_eq!(image.tag(), NITRO_TAG);
    }

    #[test]
    fn test_custom_tag() {
        let image = NitroSequencerImage::new().with_tag("v3.0.0");
        assert_eq!(image.tag(), "v3.0.0");
    }

    #[test]
    fn test_exposes_ports() {
        let image = NitroSequencerImage::new();
        let ports = image.expose_ports();
        assert!(ports.contains(&ContainerPort::Tcp(SEQUENCER_RPC_PORT)));
        assert!(ports.contains(&ContainerPort::Tcp(SEQUENCER_WS_PORT)));
    }

    #[test]
    fn test_cmd_includes_dev_mode() {
        let image = NitroSequencerImage::new();
        let cmd: Vec<String> = image
            .cmd()
            .into_iter()
            .map(|s| s.into().into_owned())
            .collect();
        assert!(cmd.contains(&"--dev".to_string()));
    }
}
