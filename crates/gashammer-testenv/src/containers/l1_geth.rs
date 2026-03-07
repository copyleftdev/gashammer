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

//! L1 Geth container definition for Nitro devnet.
//!
//! Runs Geth in dev mode with instant finality and 1-second block time.
//! See RFC-0013 §2.2.

use testcontainers::core::{ContainerPort, WaitFor};
use testcontainers::Image;

/// Default L1 Geth Docker image.
pub const L1_GETH_IMAGE: &str = "ethereum/client-go";

/// Default L1 Geth image tag.
pub const L1_GETH_TAG: &str = "stable";

/// Default L1 RPC port inside the container.
pub const L1_RPC_PORT: u16 = 8545;

/// L1 Geth container for Nitro devnet.
///
/// Runs Geth in `--dev` mode with:
/// - Instant block finality
/// - 1-second block period
/// - HTTP RPC on port 8545
/// - No authentication
#[derive(Debug, Clone)]
pub struct L1GethImage {
    tag: String,
}

impl Default for L1GethImage {
    fn default() -> Self {
        Self {
            tag: L1_GETH_TAG.to_string(),
        }
    }
}

impl L1GethImage {
    /// Create with a specific image tag.
    pub fn with_tag(tag: impl Into<String>) -> Self {
        Self { tag: tag.into() }
    }
}

impl Image for L1GethImage {
    fn name(&self) -> &str {
        L1_GETH_IMAGE
    }

    fn tag(&self) -> &str {
        &self.tag
    }

    fn ready_conditions(&self) -> Vec<WaitFor> {
        vec![WaitFor::message_on_stderr("HTTP server started")]
    }

    fn expose_ports(&self) -> &[ContainerPort] {
        &[ContainerPort::Tcp(L1_RPC_PORT)]
    }

    fn cmd(&self) -> impl IntoIterator<Item = impl Into<std::borrow::Cow<'_, str>>> {
        vec![
            "--dev",
            "--dev.period=1",
            "--http",
            "--http.addr=0.0.0.0",
            "--http.port=8545",
            "--http.api=eth,net,web3,debug,admin,personal,miner,txpool",
            "--http.corsdomain=*",
            "--http.vhosts=*",
            "--verbosity=3",
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_image() {
        let image = L1GethImage::default();
        assert_eq!(image.name(), L1_GETH_IMAGE);
        assert_eq!(image.tag(), L1_GETH_TAG);
    }

    #[test]
    fn test_custom_tag() {
        let image = L1GethImage::with_tag("v1.13.0");
        assert_eq!(image.tag(), "v1.13.0");
    }

    #[test]
    fn test_exposes_rpc_port() {
        let image = L1GethImage::default();
        let ports = image.expose_ports();
        assert!(ports.contains(&ContainerPort::Tcp(L1_RPC_PORT)));
    }

    #[test]
    fn test_ready_condition() {
        let image = L1GethImage::default();
        let conditions = image.ready_conditions();
        assert!(!conditions.is_empty());
    }

    #[test]
    fn test_cmd_includes_dev_mode() {
        let image = L1GethImage::default();
        let cmd: Vec<String> = image
            .cmd()
            .into_iter()
            .map(|s| s.into().into_owned())
            .collect();
        assert!(cmd.contains(&"--dev".to_string()));
        assert!(cmd.contains(&"--dev.period=1".to_string()));
    }
}
