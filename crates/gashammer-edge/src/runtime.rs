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

//! Edge runtime — startup, heartbeat, and shutdown management.
//!
//! Ref: RFC-0004 §2–4.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use tokio::sync::Notify;
use tokio::task::JoinHandle;

use gashammer_common::build_info;
use gashammer_common::EdgeId;

use crate::config::EdgeConfig;
use crate::error::EdgeError;

// ---------------------------------------------------------------------------
// Runtime state
// ---------------------------------------------------------------------------

/// Current state of the edge runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeState {
    /// Initializing — parsing config, establishing connections.
    Starting,
    /// Registered with hive, awaiting workload.
    Idle,
    /// Executing a workload.
    Running,
    /// Shutting down — flushing telemetry, deregistering.
    ShuttingDown,
    /// Stopped.
    Stopped,
}

/// Metrics tracked by the edge runtime.
#[derive(Debug, Default)]
pub struct RuntimeMetrics {
    pub gashammer_heartbeats_sent: AtomicU64,
    pub gashammer_connection_errors: AtomicU64,
}

impl RuntimeMetrics {
    pub fn heartbeats_sent(&self) -> u64 {
        self.gashammer_heartbeats_sent.load(Ordering::Relaxed)
    }

    pub fn connection_errors(&self) -> u64 {
        self.gashammer_connection_errors.load(Ordering::Relaxed)
    }
}

/// The edge runtime manages the lifecycle of the edge process.
pub struct EdgeRuntime {
    config: EdgeConfig,
    edge_id: Option<EdgeId>,
    state: RuntimeState,
    shutdown: Arc<Notify>,
    is_shutdown: Arc<AtomicBool>,
    metrics: Arc<RuntimeMetrics>,
}

impl EdgeRuntime {
    /// Create a new edge runtime from configuration.
    pub fn new(config: EdgeConfig) -> Self {
        Self {
            config,
            edge_id: None,
            state: RuntimeState::Starting,
            shutdown: Arc::new(Notify::new()),
            is_shutdown: Arc::new(AtomicBool::new(false)),
            metrics: Arc::new(RuntimeMetrics::default()),
        }
    }

    /// Returns the edge ID (assigned after registration).
    pub fn edge_id(&self) -> Option<&EdgeId> {
        self.edge_id.as_ref()
    }

    /// Returns the current runtime state.
    pub fn state(&self) -> &RuntimeState {
        &self.state
    }

    /// Returns a reference to runtime metrics.
    pub fn metrics(&self) -> &Arc<RuntimeMetrics> {
        &self.metrics
    }

    /// Returns the configuration.
    pub fn config(&self) -> &EdgeConfig {
        &self.config
    }

    /// Log the startup banner with version and copyright (DNA).
    pub fn log_startup_banner(&self) {
        let info = build_info::build_info();
        tracing::info!(
            name = info.name,
            version = info.version,
            build_sha = info.build_sha,
            region = self.config.region,
            sequencer = self.config.sequencer.url,
            "{} v{} starting — {} — {}",
            info.name,
            info.version,
            info.copyright,
            info.url,
        );
    }

    /// Print version information (for the `version` subcommand).
    pub fn print_version() -> String {
        build_info::version_banner()
    }

    /// Verify that the sequencer RPC is reachable.
    pub async fn check_sequencer_connection(&self) -> Result<u64, EdgeError> {
        let rpc_config = self.config.sequencer_rpc_config();
        let provider = gashammer_nitro::rpc::NitroRpcProvider::new(rpc_config)
            .map_err(|e| EdgeError::Connection(format!("sequencer provider: {e}")))?;
        provider
            .health_check()
            .await
            .map_err(|e| EdgeError::Connection(format!("sequencer health check: {e}")))
    }

    /// Set the edge as registered with the given ID.
    pub fn set_registered(&mut self, edge_id: EdgeId) {
        self.edge_id = Some(edge_id);
        self.state = RuntimeState::Idle;
    }

    /// Transition to Running state.
    pub fn set_running(&mut self) {
        self.state = RuntimeState::Running;
    }

    /// Start the heartbeat task. Returns a JoinHandle that must be stored.
    pub fn start_heartbeat(&self) -> JoinHandle<()> {
        let interval = self.config.heartbeat_interval();
        let shutdown = Arc::clone(&self.shutdown);
        let is_shutdown = Arc::clone(&self.is_shutdown);
        let metrics = Arc::clone(&self.metrics);

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        if is_shutdown.load(Ordering::Relaxed) {
                            tracing::info!("heartbeat task stopping (shutdown)");
                            break;
                        }
                        // In real impl, this sends a gRPC heartbeat to hive.
                        // For now, just increment the counter.
                        metrics
                            .gashammer_heartbeats_sent
                            .fetch_add(1, Ordering::Relaxed);
                        tracing::trace!("heartbeat sent");
                    }
                    _ = shutdown.notified() => {
                        tracing::info!("heartbeat task received shutdown signal");
                        break;
                    }
                }
            }
        })
    }

    /// Trigger graceful shutdown.
    pub fn trigger_shutdown(&mut self) {
        self.state = RuntimeState::ShuttingDown;
        self.is_shutdown.store(true, Ordering::Relaxed);
        self.shutdown.notify_waiters();
        tracing::info!("shutdown triggered");
    }

    /// Wait for shutdown signal (for external callers).
    pub async fn wait_for_shutdown(&self) {
        self.shutdown.notified().await;
    }

    /// Returns a clone of the shutdown Notify for signal handlers.
    pub fn shutdown_notify(&self) -> Arc<Notify> {
        Arc::clone(&self.shutdown)
    }

    /// Mark runtime as fully stopped.
    pub fn set_stopped(&mut self) {
        self.state = RuntimeState::Stopped;
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn test_config() -> EdgeConfig {
        crate::config::EdgeConfig::from_toml(
            r#"
            region = "us-west-2"
            hive_endpoint = "http://localhost:9090"
            bootstrap_token = "test"
            [sequencer]
            url = "http://localhost:8547"
            "#,
        )
        .expect("valid test config")
    }

    #[test]
    fn test_runtime_starts_in_starting_state() {
        let rt = EdgeRuntime::new(test_config());
        assert_eq!(*rt.state(), RuntimeState::Starting);
        assert!(rt.edge_id().is_none());
    }

    #[test]
    fn test_set_registered_transitions_to_idle() {
        let mut rt = EdgeRuntime::new(test_config());
        rt.set_registered(EdgeId::new());
        assert_eq!(*rt.state(), RuntimeState::Idle);
        assert!(rt.edge_id().is_some());
    }

    #[test]
    fn test_set_running() {
        let mut rt = EdgeRuntime::new(test_config());
        rt.set_registered(EdgeId::new());
        rt.set_running();
        assert_eq!(*rt.state(), RuntimeState::Running);
    }

    #[test]
    fn test_trigger_shutdown() {
        let mut rt = EdgeRuntime::new(test_config());
        rt.trigger_shutdown();
        assert_eq!(*rt.state(), RuntimeState::ShuttingDown);
    }

    #[test]
    fn test_set_stopped() {
        let mut rt = EdgeRuntime::new(test_config());
        rt.trigger_shutdown();
        rt.set_stopped();
        assert_eq!(*rt.state(), RuntimeState::Stopped);
    }

    #[test]
    fn test_version_banner() {
        let banner = EdgeRuntime::print_version();
        assert!(banner.contains("GasHammer"));
        assert!(banner.contains("BSL-1.1"));
    }

    #[tokio::test]
    async fn test_heartbeat_task_increments_counter() {
        let config = crate::config::EdgeConfig::from_toml(
            r#"
            region = "test"
            hive_endpoint = "http://localhost:9090"
            bootstrap_token = "test"
            heartbeat_interval_ms = 50
            [sequencer]
            url = "http://localhost:8547"
            "#,
        )
        .unwrap();

        let mut rt = EdgeRuntime::new(config);
        let handle = rt.start_heartbeat();

        // Let a few heartbeats fire
        tokio::time::sleep(Duration::from_millis(160)).await;

        rt.trigger_shutdown();
        handle.await.expect("heartbeat task should complete");

        assert!(rt.metrics().heartbeats_sent() >= 2);
    }

    #[tokio::test]
    async fn test_shutdown_stops_heartbeat() {
        let config = crate::config::EdgeConfig::from_toml(
            r#"
            region = "test"
            hive_endpoint = "http://localhost:9090"
            bootstrap_token = "test"
            heartbeat_interval_ms = 10
            [sequencer]
            url = "http://localhost:8547"
            "#,
        )
        .unwrap();

        let mut rt = EdgeRuntime::new(config);
        let handle = rt.start_heartbeat();

        // Immediate shutdown
        rt.trigger_shutdown();
        let result = tokio::time::timeout(Duration::from_secs(1), handle).await;
        assert!(result.is_ok(), "heartbeat should stop within timeout");
    }

    #[test]
    fn test_config_accessor() {
        let rt = EdgeRuntime::new(test_config());
        assert_eq!(rt.config().region, "us-west-2");
    }
}
