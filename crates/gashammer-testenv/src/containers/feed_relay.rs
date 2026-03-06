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

//! Feed relay container definition.
//!
//! Runs the Nitro node in relay mode, subscribing to the sequencer's
//! feed output and redistributing to clients on port 9642.
//! See RFC-0013 §2.2.

use testcontainers::core::{ContainerPort, WaitFor};
use testcontainers::Image;

use super::sequencer::NITRO_IMAGE;

/// Default feed relay image tag.
pub const FEED_RELAY_TAG: &str = "latest";

/// Feed relay WebSocket port.
pub const FEED_RELAY_PORT: u16 = 9642;

/// Feed relay container.
///
/// Runs the Nitro node in relay mode:
/// - Subscribes to sequencer feed
/// - Exposes WebSocket feed on port 9642
#[derive(Debug, Clone)]
pub struct FeedRelayImage {
    tag: String,
    sequencer_feed_url: String,
}

impl FeedRelayImage {
    /// Create a feed relay pointing at the sequencer's feed output.
    pub fn new(sequencer_feed_url: impl Into<String>) -> Self {
        Self {
            tag: FEED_RELAY_TAG.to_string(),
            sequencer_feed_url: sequencer_feed_url.into(),
        }
    }

    /// Use a specific image tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = tag.into();
        self
    }
}

impl Image for FeedRelayImage {
    fn name(&self) -> &str {
        NITRO_IMAGE
    }

    fn tag(&self) -> &str {
        &self.tag
    }

    fn ready_conditions(&self) -> Vec<WaitFor> {
        vec![WaitFor::message_on_stdout("Started HTTP server")]
    }

    fn expose_ports(&self) -> &[ContainerPort] {
        &[ContainerPort::Tcp(FEED_RELAY_PORT)]
    }

    fn cmd(&self) -> impl IntoIterator<Item = impl Into<std::borrow::Cow<'_, str>>> {
        vec![
            "relay".to_string(),
            "--node.feed.output.enable".to_string(),
            "--node.feed.output.addr=0.0.0.0".to_string(),
            format!("--node.feed.output.port={FEED_RELAY_PORT}"),
            "--node.feed.input.enable".to_string(),
            format!("--node.feed.input.url={}", self.sequencer_feed_url),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feed_relay_image() {
        let image = FeedRelayImage::new("ws://sequencer:9642");
        assert_eq!(image.name(), NITRO_IMAGE);
        assert_eq!(image.tag(), FEED_RELAY_TAG);
    }

    #[test]
    fn test_custom_tag() {
        let image = FeedRelayImage::new("ws://seq:9642").with_tag("v3.0.0");
        assert_eq!(image.tag(), "v3.0.0");
    }

    #[test]
    fn test_exposes_feed_port() {
        let image = FeedRelayImage::new("ws://seq:9642");
        let ports = image.expose_ports();
        assert!(ports.contains(&ContainerPort::Tcp(FEED_RELAY_PORT)));
    }

    #[test]
    fn test_cmd_includes_relay_mode() {
        let image = FeedRelayImage::new("ws://seq:9642");
        let cmd: Vec<String> = image
            .cmd()
            .into_iter()
            .map(|s| s.into().into_owned())
            .collect();
        assert!(cmd.contains(&"relay".to_string()));
        assert!(cmd.iter().any(|s| s.contains("ws://seq:9642")));
    }
}
