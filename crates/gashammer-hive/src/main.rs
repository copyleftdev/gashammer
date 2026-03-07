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

//! GasHammer Hive — control plane process.
//!
//! Boots the REST API server, manages the edge registry and run orchestrator.
//! Ref: RFC-0003.

use std::sync::Arc;

use clap::Parser;

use gashammer_common::build_info::GASHAMMER_CANARY;
use gashammer_hive::api::{router, AppState};
use gashammer_hive::registry::{EdgeRegistry, RegistryConfig};
use gashammer_hive::run::RunOrchestrator;

/// Force the canary string into the binary.
#[used]
static _CANARY: &str = GASHAMMER_CANARY;

/// GasHammer Hive — control plane server.
#[derive(Parser)]
#[command(name = "gashammer-hive", version, about)]
struct Cli {
    /// Listen address for the REST API.
    #[arg(long, default_value = "0.0.0.0:8080", env = "GASHAMMER_LISTEN")]
    listen: String,

    /// API key for Bearer authentication (empty = no auth).
    #[arg(long, default_value = "", env = "GASHAMMER_API_KEY")]
    api_key: String,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run(cli))
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    tracing::info!(
        version = gashammer_common::build_info::GASHAMMER_VERSION,
        "GasHammer Hive starting"
    );

    // Boot registry with reaper
    let registry = EdgeRegistry::new(RegistryConfig::default());
    let reaper_handle = registry.start_reaper();

    // Boot run orchestrator
    let orchestrator = RunOrchestrator::new();

    // Build shared state and router
    let state = Arc::new(AppState::new(registry, orchestrator, cli.api_key));
    let app = router(state);

    // Bind and serve
    let listener = tokio::net::TcpListener::bind(&cli.listen).await?;
    tracing::info!(listen = %cli.listen, "REST API listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    reaper_handle.abort();
    tracing::info!("hive shutdown complete");
    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install CTRL+C handler");
    tracing::info!("SIGINT received, shutting down");
}
