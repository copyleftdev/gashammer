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

//! GasHammer Edge — regional worker process.
//!
//! Boots the edge runtime, connects to the sequencer, generates workload
//! transactions, and submits them via RPC. Ref: RFC-0004.

use std::sync::Arc;
use std::time::Instant;

use clap::Parser;
use tokio::sync::mpsc;

use gashammer_common::EdgeId;
use gashammer_edge::cli::{Cli, Command};
use gashammer_edge::config::EdgeConfig;
use gashammer_edge::pipeline::{BuiltTx, PipelineConfig, TxPipeline};
use gashammer_edge::runtime::EdgeRuntime;
use gashammer_workload::account::AccountPool;
use gashammer_workload::engine::{
    EngineConfig, GasRateMode, RateLimitMode, WeightedTemplate, WorkloadEngine, TICK_INTERVAL_MS,
};
use gashammer_workload::templates::builtin_templates;

fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    // Handle version subcommand
    if let Some(Command::Version) = &cli.command {
        println!("{}", EdgeRuntime::print_version());
        return Ok(());
    }

    // Load config
    let config = if let Some(ref path) = cli.config {
        EdgeConfig::from_file(path)?
    } else {
        EdgeConfig::from_toml(
            r#"
            region = "local"
            hive_endpoint = "http://localhost:9090"
            bootstrap_token = "dev"
            [sequencer]
            url = "http://localhost:8547"
            "#,
        )?
    };

    let config = cli.apply_overrides(config);

    // Boot tokio runtime
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run(config))
}

async fn run(config: EdgeConfig) -> anyhow::Result<()> {
    let mut runtime = EdgeRuntime::new(config.clone());
    runtime.log_startup_banner();

    // 1. Check sequencer connectivity
    let chain_id = runtime.check_sequencer_connection().await?;
    tracing::info!(chain_id, "sequencer connection verified");

    // 2. Self-register (standalone mode — no hive gRPC yet)
    let edge_id = EdgeId::new();
    let edge_id_str = edge_id.to_string();
    runtime.set_registered(edge_id);
    tracing::info!(edge_id = %edge_id_str, "edge registered (standalone mode)");

    // 3. Start heartbeat
    let heartbeat_handle = runtime.start_heartbeat();

    // 4. Set up account pool (deterministic from seed)
    let account_pool = AccountPool::from_seed(42, 0, 10);
    tracing::info!(accounts = account_pool.len(), "account pool initialized");

    // 5. Set up transaction pipeline
    let (event_tx, mut event_rx) = mpsc::channel(1024);
    let pipeline_config = PipelineConfig {
        concurrency: 4,
        buffer_size: 1024,
        chain_id,
    };
    let mut pipeline = TxPipeline::new(pipeline_config, event_tx);
    let tx_sender = pipeline.tx_sender();
    let pipeline_metrics = Arc::clone(pipeline.metrics());

    let rpc_url = config.sequencer.url.clone();
    let worker_handles = pipeline.start(rpc_url)?;
    tracing::info!(
        workers = worker_handles.len(),
        "transaction pipeline started"
    );

    // 6. Set up workload engine
    let templates = builtin_templates();
    let weights: Vec<WeightedTemplate> = templates
        .iter()
        .enumerate()
        .map(|(i, t)| WeightedTemplate {
            template_name: t.descriptor().name.clone(),
            weight: if i == 0 { 1.0 } else { 0.0 }, // SimpleTransfer only for now
        })
        .collect();

    let engine_config = EngineConfig {
        gas_rate: GasRateMode::Sustained {
            gas_per_sec: 50_000,
        },
        rate_limit_mode: RateLimitMode::GasPerSecond,
        templates: weights,
        seed: 42,
    };
    let mut engine = WorkloadEngine::new(engine_config, templates);

    // 7. Run the main loop
    runtime.set_running();
    tracing::info!("edge runtime entering main loop");

    let shutdown = runtime.shutdown_notify();
    let start = Instant::now();
    let mut tick_interval =
        tokio::time::interval(std::time::Duration::from_millis(TICK_INTERVAL_MS));

    // Spawn event drain task
    let event_drain_handle = tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            match event {
                gashammer_edge::pipeline::PipelineEvent::TxSubmitted {
                    tx_hash,
                    estimated_gas,
                    ..
                } => {
                    tracing::debug!(%tx_hash, estimated_gas, "tx submitted");
                }
                gashammer_edge::pipeline::PipelineEvent::TxFailed { error, .. } => {
                    tracing::warn!(error, "tx failed");
                }
            }
        }
    });

    loop {
        tokio::select! {
            _ = tick_interval.tick() => {
                let elapsed_ms = start.elapsed().as_millis() as u64;
                let target = account_pool.next_account();
                let output = engine.tick(elapsed_ms, target.address());

                for template_output in output.transactions {
                    let sender_account = account_pool.next_account();
                    let nonce = sender_account.next_nonce();

                    let built_tx = BuiltTx {
                        to: template_output.to,
                        value: template_output.value,
                        data: template_output.data,
                        gas_limit: template_output.gas_limit,
                        nonce,
                        chain_id,
                        estimated_gas: template_output.estimated_gas,
                        template_name: template_output.template_name,
                    };

                    if tx_sender.try_send(built_tx).is_err() {
                        tracing::warn!("pipeline backpressure — dropping tx");
                    }
                }
            }
            _ = shutdown.notified() => {
                tracing::info!("shutdown signal received, draining pipeline");
                break;
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("SIGINT received, shutting down");
                break;
            }
        }
    }

    // Graceful shutdown
    drop(tx_sender);
    drop(pipeline);

    for h in worker_handles {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), h).await;
    }

    event_drain_handle.abort();
    heartbeat_handle.abort();

    tracing::info!(
        submitted = pipeline_metrics.submitted(),
        failed = pipeline_metrics.failed(),
        total_gas = pipeline_metrics.total_gas(),
        "edge shutdown complete"
    );

    Ok(())
}
