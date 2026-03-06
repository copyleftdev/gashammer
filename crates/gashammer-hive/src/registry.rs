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

//! Edge registry — registration, heartbeat, health tracking, and reaper.
//!
//! Ref: RFC-0003 §3.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use gashammer_common::EdgeId;

use crate::error::HiveError;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the edge registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryConfig {
    /// Expected heartbeat interval from edges (default: 5s).
    pub heartbeat_interval_ms: u64,
    /// Time after which a healthy edge becomes degraded (default: 15s = 3 missed beats).
    pub degraded_threshold_ms: u64,
    /// Time after which an edge goes offline (default: 30s = 6 missed beats).
    pub offline_threshold_ms: u64,
    /// How often the reaper sweeps for stale edges (default: 5s).
    pub reaper_interval_ms: u64,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval_ms: 5_000,
            degraded_threshold_ms: 15_000,
            offline_threshold_ms: 30_000,
            reaper_interval_ms: 5_000,
        }
    }
}

// ---------------------------------------------------------------------------
// Edge data model
// ---------------------------------------------------------------------------

/// Edge lifecycle states per RFC-0003 §3.4.
///
/// ```text
///   Registering ──► Healthy ──► Draining ──► Offline
///                     │   ▲         │
///                     ▼   │         │
///                  Degraded ────────┘
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeStatus {
    Registering,
    Healthy,
    Degraded { reason: String },
    Draining,
    Offline,
}

impl EdgeStatus {
    /// Returns `true` if the edge is available for work assignment.
    pub fn is_available(&self) -> bool {
        matches!(self, EdgeStatus::Healthy)
    }

    /// Returns `true` if the edge is connected (not offline).
    pub fn is_connected(&self) -> bool {
        !matches!(self, EdgeStatus::Offline)
    }

    /// Validate a state transition per RFC-0003 §3.4.
    pub(crate) fn can_transition_to(&self, target: &EdgeStatus) -> bool {
        match (self, target) {
            // Registering can only go to Healthy
            (EdgeStatus::Registering, EdgeStatus::Healthy) => true,
            // Healthy can go to Degraded, Draining, or Offline
            (EdgeStatus::Healthy, EdgeStatus::Degraded { .. }) => true,
            (EdgeStatus::Healthy, EdgeStatus::Draining) => true,
            (EdgeStatus::Healthy, EdgeStatus::Offline) => true,
            // Degraded can go back to Healthy, or to Draining/Offline
            (EdgeStatus::Degraded { .. }, EdgeStatus::Healthy) => true,
            (EdgeStatus::Degraded { .. }, EdgeStatus::Draining) => true,
            (EdgeStatus::Degraded { .. }, EdgeStatus::Offline) => true,
            // Draining goes to Offline
            (EdgeStatus::Draining, EdgeStatus::Offline) => true,
            _ => false,
        }
    }
}

/// Edge capacity classification per RFC-0003 §3.1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CapacityClass {
    Small,
    Medium,
    Large,
    Custom { cores: u32, memory_mb: u64 },
}

/// Hardware profile reported by the edge at registration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceProfile {
    pub cpu_cores: u32,
    pub memory_mb: u64,
    pub network_class: String,
}

/// Heartbeat payload from an edge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatPayload {
    pub cpu_utilization: f32,
    pub memory_utilization: f32,
    pub active_connections: u32,
    pub current_run_id: Option<String>,
}

/// Registration request from an edge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterEdgeRequest {
    pub bootstrap_token: String,
    pub region: String,
    pub capacity_class: CapacityClass,
    pub software_version: String,
    pub supported_adapters: Vec<String>,
    pub instance_profile: InstanceProfile,
}

/// A registered edge record per RFC-0003 §3.1.
#[derive(Debug, Clone)]
pub struct EdgeRecord {
    pub edge_id: EdgeId,
    pub region: String,
    pub capacity_class: CapacityClass,
    pub software_version: String,
    pub supported_adapters: Vec<String>,
    pub instance_profile: InstanceProfile,
    pub status: EdgeStatus,
    pub last_heartbeat: Instant,
    pub registered_at: Instant,
    pub last_payload: Option<HeartbeatPayload>,
}

// ---------------------------------------------------------------------------
// Metrics (atomic counters)
// ---------------------------------------------------------------------------

/// Metrics for the edge registry.
#[derive(Debug, Default)]
pub struct RegistryMetrics {
    pub gashammer_edges_registered: std::sync::atomic::AtomicU64,
    pub gashammer_edges_active: std::sync::atomic::AtomicU64,
    pub gashammer_heartbeat_count: std::sync::atomic::AtomicU64,
}

impl RegistryMetrics {
    fn inc_registered(&self) {
        self.gashammer_edges_registered
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    fn dec_registered(&self) {
        self.gashammer_edges_registered
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }

    fn set_active(&self, count: u64) {
        self.gashammer_edges_active
            .store(count, std::sync::atomic::Ordering::Relaxed);
    }

    fn inc_heartbeat(&self) {
        self.gashammer_heartbeat_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn registered_count(&self) -> u64 {
        self.gashammer_edges_registered
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn active_count(&self) -> u64 {
        self.gashammer_edges_active
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn heartbeat_count(&self) -> u64 {
        self.gashammer_heartbeat_count
            .load(std::sync::atomic::Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------------
// EdgeRegistry
// ---------------------------------------------------------------------------

/// Concurrent-safe edge registry.
///
/// Manages edge registration, heartbeats, status transitions, and a
/// background reaper that marks stale edges as Degraded/Offline.
pub struct EdgeRegistry {
    edges: Arc<RwLock<HashMap<EdgeId, EdgeRecord>>>,
    config: RegistryConfig,
    metrics: Arc<RegistryMetrics>,
}

impl EdgeRegistry {
    /// Create a new edge registry with the given configuration.
    pub fn new(config: RegistryConfig) -> Self {
        Self {
            edges: Arc::new(RwLock::new(HashMap::new())),
            config,
            metrics: Arc::new(RegistryMetrics::default()),
        }
    }

    /// Returns a reference to the registry metrics.
    pub fn metrics(&self) -> &Arc<RegistryMetrics> {
        &self.metrics
    }

    /// Register a new edge. Returns the assigned `EdgeId`.
    #[tracing::instrument(skip(self, request), fields(region = %request.region))]
    pub async fn register(&self, request: RegisterEdgeRequest) -> Result<EdgeId, HiveError> {
        let edge_id = EdgeId::new();
        let now = Instant::now();

        let record = EdgeRecord {
            edge_id: edge_id.clone(),
            region: request.region,
            capacity_class: request.capacity_class,
            software_version: request.software_version,
            supported_adapters: request.supported_adapters,
            instance_profile: request.instance_profile,
            status: EdgeStatus::Registering,
            last_heartbeat: now,
            registered_at: now,
            last_payload: None,
        };

        let mut edges = self.edges.write().await;
        edges.insert(edge_id.clone(), record);
        self.metrics.inc_registered();

        tracing::info!(%edge_id, "edge registered");
        Ok(edge_id)
    }

    /// Process a heartbeat from an edge. Transitions Registering → Healthy
    /// and Degraded → Healthy as appropriate.
    #[tracing::instrument(skip(self, payload), fields(%edge_id))]
    pub async fn heartbeat(
        &self,
        edge_id: &EdgeId,
        payload: HeartbeatPayload,
    ) -> Result<(), HiveError> {
        let mut edges = self.edges.write().await;
        let record = edges
            .get_mut(edge_id)
            .ok_or_else(|| HiveError::EdgeRegistry(format!("unknown edge: {edge_id}")))?;

        record.last_heartbeat = Instant::now();
        record.last_payload = Some(payload);
        self.metrics.inc_heartbeat();

        // State transitions on heartbeat
        match &record.status {
            EdgeStatus::Registering => {
                record.status = EdgeStatus::Healthy;
                tracing::info!(%edge_id, "edge transitioned to Healthy");
            }
            EdgeStatus::Degraded { .. } => {
                record.status = EdgeStatus::Healthy;
                tracing::info!(%edge_id, "edge recovered to Healthy");
            }
            EdgeStatus::Healthy => {}
            EdgeStatus::Draining | EdgeStatus::Offline => {
                tracing::warn!(
                    %edge_id,
                    status = ?record.status,
                    "heartbeat from edge in terminal state, ignoring"
                );
            }
        }

        Ok(())
    }

    /// Deregister an edge, removing it from the registry.
    #[tracing::instrument(skip(self), fields(%edge_id))]
    pub async fn deregister(&self, edge_id: &EdgeId) -> Result<(), HiveError> {
        let mut edges = self.edges.write().await;
        if edges.remove(edge_id).is_some() {
            self.metrics.dec_registered();
            tracing::info!(%edge_id, "edge deregistered");
            Ok(())
        } else {
            Err(HiveError::EdgeRegistry(format!("unknown edge: {edge_id}")))
        }
    }

    /// Request an edge to start draining (graceful removal).
    #[tracing::instrument(skip(self), fields(%edge_id))]
    pub async fn drain(&self, edge_id: &EdgeId) -> Result<(), HiveError> {
        let mut edges = self.edges.write().await;
        let record = edges
            .get_mut(edge_id)
            .ok_or_else(|| HiveError::EdgeRegistry(format!("unknown edge: {edge_id}")))?;

        if !record.status.can_transition_to(&EdgeStatus::Draining) {
            return Err(HiveError::EdgeRegistry(format!(
                "cannot drain edge {edge_id} in state {:?}",
                record.status
            )));
        }

        record.status = EdgeStatus::Draining;
        tracing::info!(%edge_id, "edge set to draining");
        Ok(())
    }

    /// Get a snapshot of a single edge record.
    pub async fn get(&self, edge_id: &EdgeId) -> Option<EdgeRecord> {
        let edges = self.edges.read().await;
        edges.get(edge_id).cloned()
    }

    /// List all registered edges.
    pub async fn list(&self) -> Vec<EdgeRecord> {
        let edges = self.edges.read().await;
        edges.values().cloned().collect()
    }

    /// List edges that are available for work assignment (status = Healthy).
    pub async fn list_available(&self) -> Vec<EdgeRecord> {
        let edges = self.edges.read().await;
        edges
            .values()
            .filter(|e| e.status.is_available())
            .cloned()
            .collect()
    }

    /// Count of currently registered edges.
    pub async fn count(&self) -> usize {
        let edges = self.edges.read().await;
        edges.len()
    }

    /// Start the background reaper task that marks stale edges as
    /// Degraded or Offline based on heartbeat timeouts.
    ///
    /// Returns a `JoinHandle` that must be stored (per project conventions).
    pub fn start_reaper(&self) -> JoinHandle<()> {
        let edges = Arc::clone(&self.edges);
        let metrics = Arc::clone(&self.metrics);
        let degraded_threshold = Duration::from_millis(self.config.degraded_threshold_ms);
        let offline_threshold = Duration::from_millis(self.config.offline_threshold_ms);
        let interval = Duration::from_millis(self.config.reaper_interval_ms);

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;

                let mut edges = edges.write().await;
                let now = Instant::now();
                let mut active_count: u64 = 0;

                for record in edges.values_mut() {
                    let elapsed = now.duration_since(record.last_heartbeat);

                    match &record.status {
                        EdgeStatus::Healthy if elapsed >= offline_threshold => {
                            tracing::warn!(
                                edge_id = %record.edge_id,
                                elapsed_ms = elapsed.as_millis() as u64,
                                "edge offline — missed heartbeats"
                            );
                            record.status = EdgeStatus::Offline;
                        }
                        EdgeStatus::Healthy if elapsed >= degraded_threshold => {
                            tracing::warn!(
                                edge_id = %record.edge_id,
                                elapsed_ms = elapsed.as_millis() as u64,
                                "edge degraded — missed heartbeats"
                            );
                            record.status = EdgeStatus::Degraded {
                                reason: "missed heartbeats".into(),
                            };
                        }
                        EdgeStatus::Degraded { .. } if elapsed >= offline_threshold => {
                            tracing::warn!(
                                edge_id = %record.edge_id,
                                elapsed_ms = elapsed.as_millis() as u64,
                                "degraded edge went offline"
                            );
                            record.status = EdgeStatus::Offline;
                        }
                        EdgeStatus::Registering if elapsed >= offline_threshold => {
                            tracing::warn!(
                                edge_id = %record.edge_id,
                                "registering edge never sent heartbeat — marking offline"
                            );
                            record.status = EdgeStatus::Offline;
                        }
                        _ => {}
                    }

                    if record.status.is_available() {
                        active_count += 1;
                    }
                }

                metrics.set_active(active_count);
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_request() -> RegisterEdgeRequest {
        RegisterEdgeRequest {
            bootstrap_token: "test-token".into(),
            region: "us-west-2".into(),
            capacity_class: CapacityClass::Small,
            software_version: "0.1.0".into(),
            supported_adapters: vec!["nitro-rpc".into()],
            instance_profile: InstanceProfile {
                cpu_cores: 4,
                memory_mb: 8192,
                network_class: "standard".into(),
            },
        }
    }

    fn default_payload() -> HeartbeatPayload {
        HeartbeatPayload {
            cpu_utilization: 0.5,
            memory_utilization: 0.3,
            active_connections: 2,
            current_run_id: None,
        }
    }

    #[tokio::test]
    async fn test_register_creates_edge_in_registering_state() {
        let registry = EdgeRegistry::new(RegistryConfig::default());
        let edge_id = registry.register(default_request()).await.unwrap();

        let record = registry.get(&edge_id).await.unwrap();
        assert_eq!(record.status, EdgeStatus::Registering);
        assert_eq!(record.region, "us-west-2");
        assert_eq!(record.capacity_class, CapacityClass::Small);
    }

    #[tokio::test]
    async fn test_heartbeat_transitions_registering_to_healthy() {
        let registry = EdgeRegistry::new(RegistryConfig::default());
        let edge_id = registry.register(default_request()).await.unwrap();

        registry
            .heartbeat(&edge_id, default_payload())
            .await
            .unwrap();

        let record = registry.get(&edge_id).await.unwrap();
        assert_eq!(record.status, EdgeStatus::Healthy);
    }

    #[tokio::test]
    async fn test_heartbeat_unknown_edge_returns_error() {
        let registry = EdgeRegistry::new(RegistryConfig::default());
        let unknown = EdgeId::new();

        let result = registry.heartbeat(&unknown, default_payload()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_deregister_removes_edge() {
        let registry = EdgeRegistry::new(RegistryConfig::default());
        let edge_id = registry.register(default_request()).await.unwrap();

        assert_eq!(registry.count().await, 1);
        registry.deregister(&edge_id).await.unwrap();
        assert_eq!(registry.count().await, 0);
    }

    #[tokio::test]
    async fn test_deregister_unknown_edge_returns_error() {
        let registry = EdgeRegistry::new(RegistryConfig::default());
        let unknown = EdgeId::new();

        let result = registry.deregister(&unknown).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_drain_transitions_healthy_to_draining() {
        let registry = EdgeRegistry::new(RegistryConfig::default());
        let edge_id = registry.register(default_request()).await.unwrap();
        registry
            .heartbeat(&edge_id, default_payload())
            .await
            .unwrap();

        registry.drain(&edge_id).await.unwrap();

        let record = registry.get(&edge_id).await.unwrap();
        assert_eq!(record.status, EdgeStatus::Draining);
    }

    #[tokio::test]
    async fn test_drain_from_registering_fails() {
        let registry = EdgeRegistry::new(RegistryConfig::default());
        let edge_id = registry.register(default_request()).await.unwrap();

        let result = registry.drain(&edge_id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_available_only_returns_healthy() {
        let registry = EdgeRegistry::new(RegistryConfig::default());

        // Edge 1: Healthy
        let id1 = registry.register(default_request()).await.unwrap();
        registry.heartbeat(&id1, default_payload()).await.unwrap();

        // Edge 2: still Registering
        let _id2 = registry.register(default_request()).await.unwrap();

        let available = registry.list_available().await;
        assert_eq!(available.len(), 1);
        assert_eq!(available[0].edge_id, id1);
    }

    #[tokio::test]
    async fn test_metrics_track_registration() {
        let registry = EdgeRegistry::new(RegistryConfig::default());

        let id1 = registry.register(default_request()).await.unwrap();
        assert_eq!(registry.metrics().registered_count(), 1);

        let _id2 = registry.register(default_request()).await.unwrap();
        assert_eq!(registry.metrics().registered_count(), 2);

        registry.deregister(&id1).await.unwrap();
        assert_eq!(registry.metrics().registered_count(), 1);
    }

    #[tokio::test]
    async fn test_metrics_track_heartbeats() {
        let registry = EdgeRegistry::new(RegistryConfig::default());
        let edge_id = registry.register(default_request()).await.unwrap();

        registry
            .heartbeat(&edge_id, default_payload())
            .await
            .unwrap();
        registry
            .heartbeat(&edge_id, default_payload())
            .await
            .unwrap();

        assert_eq!(registry.metrics().heartbeat_count(), 2);
    }

    // State machine transition tests
    #[test]
    fn test_status_registering_can_only_go_to_healthy() {
        let s = EdgeStatus::Registering;
        assert!(s.can_transition_to(&EdgeStatus::Healthy));
        assert!(!s.can_transition_to(&EdgeStatus::Draining));
        assert!(!s.can_transition_to(&EdgeStatus::Offline));
    }

    #[test]
    fn test_status_healthy_transitions() {
        let s = EdgeStatus::Healthy;
        assert!(s.can_transition_to(&EdgeStatus::Degraded {
            reason: "test".into()
        }));
        assert!(s.can_transition_to(&EdgeStatus::Draining));
        assert!(s.can_transition_to(&EdgeStatus::Offline));
        assert!(!s.can_transition_to(&EdgeStatus::Registering));
    }

    #[test]
    fn test_status_degraded_transitions() {
        let s = EdgeStatus::Degraded {
            reason: "missed heartbeats".into(),
        };
        assert!(s.can_transition_to(&EdgeStatus::Healthy));
        assert!(s.can_transition_to(&EdgeStatus::Draining));
        assert!(s.can_transition_to(&EdgeStatus::Offline));
        assert!(!s.can_transition_to(&EdgeStatus::Registering));
    }

    #[test]
    fn test_status_draining_can_only_go_to_offline() {
        let s = EdgeStatus::Draining;
        assert!(s.can_transition_to(&EdgeStatus::Offline));
        assert!(!s.can_transition_to(&EdgeStatus::Healthy));
        assert!(!s.can_transition_to(&EdgeStatus::Registering));
    }

    #[test]
    fn test_status_offline_is_terminal() {
        let s = EdgeStatus::Offline;
        assert!(!s.can_transition_to(&EdgeStatus::Healthy));
        assert!(!s.can_transition_to(&EdgeStatus::Registering));
        assert!(!s.can_transition_to(&EdgeStatus::Draining));
    }

    #[tokio::test]
    async fn test_reaper_marks_stale_edge_degraded_then_offline() {
        let config = RegistryConfig {
            heartbeat_interval_ms: 10,
            degraded_threshold_ms: 50,
            offline_threshold_ms: 100,
            reaper_interval_ms: 20,
        };
        let registry = EdgeRegistry::new(config);
        let edge_id = registry.register(default_request()).await.unwrap();
        registry
            .heartbeat(&edge_id, default_payload())
            .await
            .unwrap();

        let record = registry.get(&edge_id).await.unwrap();
        assert_eq!(record.status, EdgeStatus::Healthy);

        let reaper_handle = registry.start_reaper();

        // Wait for degraded threshold
        tokio::time::sleep(Duration::from_millis(80)).await;
        let record = registry.get(&edge_id).await.unwrap();
        assert!(
            matches!(record.status, EdgeStatus::Degraded { .. }),
            "expected Degraded, got {:?}",
            record.status
        );

        // Wait for offline threshold
        tokio::time::sleep(Duration::from_millis(80)).await;
        let record = registry.get(&edge_id).await.unwrap();
        assert_eq!(record.status, EdgeStatus::Offline);

        reaper_handle.abort();
    }

    #[tokio::test]
    async fn test_heartbeat_recovers_degraded_to_healthy() {
        let config = RegistryConfig {
            heartbeat_interval_ms: 10,
            degraded_threshold_ms: 50,
            offline_threshold_ms: 200,
            reaper_interval_ms: 20,
        };
        let registry = EdgeRegistry::new(config);
        let edge_id = registry.register(default_request()).await.unwrap();
        registry
            .heartbeat(&edge_id, default_payload())
            .await
            .unwrap();

        let reaper_handle = registry.start_reaper();

        // Wait for degraded
        tokio::time::sleep(Duration::from_millis(80)).await;
        let record = registry.get(&edge_id).await.unwrap();
        assert!(matches!(record.status, EdgeStatus::Degraded { .. }));

        // Send heartbeat to recover
        registry
            .heartbeat(&edge_id, default_payload())
            .await
            .unwrap();
        let record = registry.get(&edge_id).await.unwrap();
        assert_eq!(record.status, EdgeStatus::Healthy);

        reaper_handle.abort();
    }

    #[test]
    fn test_edge_status_is_available() {
        assert!(EdgeStatus::Healthy.is_available());
        assert!(!EdgeStatus::Registering.is_available());
        assert!(!EdgeStatus::Degraded { reason: "x".into() }.is_available());
        assert!(!EdgeStatus::Draining.is_available());
        assert!(!EdgeStatus::Offline.is_available());
    }

    #[test]
    fn test_edge_status_is_connected() {
        assert!(EdgeStatus::Healthy.is_connected());
        assert!(EdgeStatus::Registering.is_connected());
        assert!(EdgeStatus::Degraded { reason: "x".into() }.is_connected());
        assert!(EdgeStatus::Draining.is_connected());
        assert!(!EdgeStatus::Offline.is_connected());
    }

    #[test]
    fn test_registry_config_default() {
        let config = RegistryConfig::default();
        assert_eq!(config.heartbeat_interval_ms, 5_000);
        assert_eq!(config.degraded_threshold_ms, 15_000);
        assert_eq!(config.offline_threshold_ms, 30_000);
        assert_eq!(config.reaper_interval_ms, 5_000);
    }

    #[test]
    fn test_capacity_class_serde_roundtrip() {
        let classes = vec![
            CapacityClass::Small,
            CapacityClass::Medium,
            CapacityClass::Large,
            CapacityClass::Custom {
                cores: 96,
                memory_mb: 384_000,
            },
        ];

        for class in &classes {
            let json = serde_json::to_string(class).expect("serialize");
            let parsed: CapacityClass = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(*class, parsed);
        }
    }
}
