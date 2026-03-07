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

//! CLI argument parsing for the edge runtime.
//!
//! Supports layered config: file → env vars → CLI flags.
//! Ref: RFC-0004 §3.1.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// GasHammer Edge — regional worker process for adversarial load testing.
#[derive(Debug, Parser)]
#[command(
    name = "gashammer-edge",
    version,
    about = "GasHammer Edge — regional worker for Nitro load testing"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Path to TOML configuration file.
    #[arg(short, long, env = "GASHAMMER_CONFIG")]
    pub config: Option<PathBuf>,

    /// Region identifier (overrides config file).
    #[arg(long, env = "GASHAMMER_REGION")]
    pub region: Option<String>,

    /// Hive gRPC endpoint (overrides config file).
    #[arg(long, env = "GASHAMMER_HIVE_ENDPOINT")]
    pub hive_endpoint: Option<String>,

    /// Bootstrap token for hive registration (overrides config file).
    #[arg(long, env = "GASHAMMER_BOOTSTRAP_TOKEN")]
    pub bootstrap_token: Option<String>,

    /// Sequencer RPC URL (overrides config file).
    #[arg(long, env = "GASHAMMER_SEQUENCER_URL")]
    pub sequencer_url: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Print version and build information.
    Version,
}

impl Cli {
    /// Apply CLI overrides to the config, producing a merged config.
    pub fn apply_overrides(
        &self,
        mut config: crate::config::EdgeConfig,
    ) -> crate::config::EdgeConfig {
        if let Some(ref region) = self.region {
            config.region = region.clone();
        }
        if let Some(ref hive_endpoint) = self.hive_endpoint {
            config.hive_endpoint = hive_endpoint.clone();
        }
        if let Some(ref token) = self.bootstrap_token {
            config.bootstrap_token = token.clone();
        }
        if let Some(ref url) = self.sequencer_url {
            config.sequencer.url = url.clone();
        }
        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use clap::CommandFactory;

    #[test]
    fn test_cli_parses_no_args() {
        // Verify the CLI struct can be constructed (validation only)
        Cli::command().debug_assert();
    }

    #[test]
    fn test_cli_version_subcommand() {
        let cli = Cli::try_parse_from(["gashammer-edge", "version"]).unwrap();
        assert!(matches!(cli.command, Some(Command::Version)));
    }

    #[test]
    fn test_cli_with_config_flag() {
        let cli = Cli::try_parse_from(["gashammer-edge", "--config", "/etc/gashammer/edge.toml"])
            .unwrap();
        assert_eq!(
            cli.config.as_deref(),
            Some(std::path::Path::new("/etc/gashammer/edge.toml"))
        );
    }

    #[test]
    fn test_cli_overrides() {
        let cli = Cli::try_parse_from([
            "gashammer-edge",
            "--region",
            "eu-central-1",
            "--hive-endpoint",
            "http://hive:9090",
            "--sequencer-url",
            "http://seq:8547",
        ])
        .unwrap();

        assert_eq!(cli.region.as_deref(), Some("eu-central-1"));
        assert_eq!(cli.hive_endpoint.as_deref(), Some("http://hive:9090"));
        assert_eq!(cli.sequencer_url.as_deref(), Some("http://seq:8547"));
    }

    #[test]
    fn test_apply_overrides() {
        let config = crate::config::EdgeConfig::from_toml(
            r#"
            region = "us-west-2"
            hive_endpoint = "http://localhost:9090"
            bootstrap_token = "test"
            [sequencer]
            url = "http://localhost:8547"
            "#,
        )
        .unwrap();

        let cli = Cli::try_parse_from([
            "gashammer-edge",
            "--region",
            "eu-central-1",
            "--sequencer-url",
            "http://custom:8547",
        ])
        .unwrap();

        let merged = cli.apply_overrides(config);
        assert_eq!(merged.region, "eu-central-1");
        assert_eq!(merged.sequencer.url, "http://custom:8547");
        // Non-overridden values preserved
        assert_eq!(merged.hive_endpoint, "http://localhost:9090");
    }
}
