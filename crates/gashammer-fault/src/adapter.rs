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

//! Fault injection adapter trait and built-in adapters.
//!
//! The `FaultAdapter` trait defines the contract for fault injection.
//! Built-in adapters implement network faults (netem, iptables),
//! feed disconnect, and RPC proxy faults.
//!
//! Ref: RFC-0008 §2–3.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Fault types
// ---------------------------------------------------------------------------

/// Types of faults that can be injected.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FaultType {
    /// Network latency (tc netem delay).
    NetworkLatency,
    /// Packet loss (tc netem loss).
    PacketLoss,
    /// Bandwidth limit (tc netem rate).
    BandwidthLimit,
    /// Network jitter (tc netem delay with variation).
    NetworkJitter,
    /// Connection reset (iptables REJECT).
    ConnectionReset,
    /// Port block (iptables DROP).
    PortBlock,
    /// WebSocket feed disconnect.
    FeedDisconnect,
    /// RPC slow response.
    RpcSlowResponse,
    /// RPC error injection.
    RpcErrorInjection,
}

/// Specification for a fault to inject.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaultSpec {
    /// Type of fault.
    pub fault_type: FaultType,
    /// Target (e.g., IP address, interface, port).
    pub target: String,
    /// Parameters specific to the fault type.
    pub parameters: HashMap<String, String>,
    /// Auto-clear after this duration (safety mechanism).
    pub duration: Option<Duration>,
}

/// Handle to an active fault.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaultHandle {
    /// Unique fault ID.
    pub id: Uuid,
    /// The fault spec.
    pub spec: FaultSpec,
    /// When the fault was injected (Unix ms).
    pub injected_at_ms: u64,
    /// Whether the fault has been cleared.
    pub cleared: bool,
}

/// Result of a preflight check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightResult {
    pub adapter_name: String,
    pub ready: bool,
    pub issues: Vec<String>,
}

// ---------------------------------------------------------------------------
// FaultAdapter trait
// ---------------------------------------------------------------------------

/// Trait for fault injection adapters.
///
/// Each adapter manages a specific class of faults and provides
/// safety guarantees (automatic cleanup on clear_all).
pub trait FaultAdapter: Send + Sync {
    /// Name of this adapter.
    fn name(&self) -> &str;

    /// Fault types supported by this adapter.
    fn supported_faults(&self) -> Vec<FaultType>;

    /// Verify prerequisites (binaries, permissions, etc.).
    fn preflight_check(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = PreflightResult> + Send + '_>>;

    /// Inject a fault. Returns a handle for tracking.
    fn inject(
        &self,
        spec: FaultSpec,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FaultHandle, String>> + Send + '_>>;

    /// Clear a specific fault by handle ID.
    fn clear(
        &self,
        handle_id: Uuid,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>>;

    /// Clear all active faults (safety: called on shutdown).
    fn clear_all(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u32, String>> + Send + '_>>;
}

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

/// Atomic metrics for fault injection.
#[derive(Debug, Default)]
pub struct FaultMetrics {
    pub gashammer_faults_injected: AtomicU64,
    pub gashammer_faults_reverted: AtomicU64,
    pub gashammer_fault_duration_seconds_sum: AtomicU64,
}

impl FaultMetrics {
    pub fn injected(&self) -> u64 {
        self.gashammer_faults_injected.load(Ordering::Relaxed)
    }

    pub fn reverted(&self) -> u64 {
        self.gashammer_faults_reverted.load(Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------------
// Fault journal
// ---------------------------------------------------------------------------

/// Entry in the fault journal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    pub timestamp_ms: u64,
    pub action: JournalAction,
    pub fault_id: Uuid,
    pub fault_type: FaultType,
    pub target: String,
}

/// Actions recorded in the journal.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalAction {
    Injected,
    Cleared,
    AutoCleared,
    EmergencyCleared,
}

/// In-memory fault journal for tracking all injection/reversion events.
#[derive(Debug, Default)]
pub struct FaultJournal {
    entries: std::sync::Mutex<Vec<JournalEntry>>,
}

impl FaultJournal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&self, entry: JournalEntry) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.push(entry);
        }
    }

    pub fn entries(&self) -> Vec<JournalEntry> {
        self.entries.lock().map(|e| e.clone()).unwrap_or_default()
    }

    pub fn len(&self) -> usize {
        self.entries.lock().map(|e| e.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// ---------------------------------------------------------------------------
// Noop adapter (for testing and dry-run mode)
// ---------------------------------------------------------------------------

/// A no-op fault adapter that simulates injection without side effects.
pub struct NoopAdapter {
    active: Arc<RwLock<HashMap<Uuid, FaultHandle>>>,
}

impl NoopAdapter {
    pub fn new() -> Self {
        Self {
            active: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for NoopAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl FaultAdapter for NoopAdapter {
    fn name(&self) -> &str {
        "noop"
    }

    fn supported_faults(&self) -> Vec<FaultType> {
        vec![
            FaultType::NetworkLatency,
            FaultType::PacketLoss,
            FaultType::BandwidthLimit,
            FaultType::NetworkJitter,
            FaultType::ConnectionReset,
            FaultType::PortBlock,
            FaultType::FeedDisconnect,
            FaultType::RpcSlowResponse,
            FaultType::RpcErrorInjection,
        ]
    }

    fn preflight_check(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = PreflightResult> + Send + '_>> {
        Box::pin(async {
            PreflightResult {
                adapter_name: "noop".into(),
                ready: true,
                issues: Vec::new(),
            }
        })
    }

    fn inject(
        &self,
        spec: FaultSpec,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FaultHandle, String>> + Send + '_>>
    {
        Box::pin(async move {
            let handle = FaultHandle {
                id: Uuid::new_v4(),
                spec,
                injected_at_ms: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
                cleared: false,
            };
            tracing::info!(fault_id = %handle.id, fault_type = ?handle.spec.fault_type, "noop: fault injected");
            self.active.write().await.insert(handle.id, handle.clone());
            Ok(handle)
        })
    }

    fn clear(
        &self,
        handle_id: Uuid,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>> {
        Box::pin(async move {
            let mut active = self.active.write().await;
            if let Some(handle) = active.remove(&handle_id) {
                tracing::info!(fault_id = %handle.id, "noop: fault cleared");
                Ok(())
            } else {
                Err(format!("fault {handle_id} not found"))
            }
        })
    }

    fn clear_all(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u32, String>> + Send + '_>> {
        Box::pin(async {
            let mut active = self.active.write().await;
            let count = active.len() as u32;
            active.clear();
            tracing::info!(count, "noop: all faults cleared");
            Ok(count)
        })
    }
}

// ---------------------------------------------------------------------------
// Fault manager
// ---------------------------------------------------------------------------

/// Manages fault lifecycle: injection, tracking, auto-clear, and journal.
pub struct FaultManager {
    adapters: Arc<Vec<Box<dyn FaultAdapter>>>,
    active: Arc<RwLock<HashMap<Uuid, (usize, FaultHandle)>>>, // (adapter_idx, handle)
    journal: Arc<FaultJournal>,
    metrics: Arc<FaultMetrics>,
}

impl FaultManager {
    pub fn new(adapters: Vec<Box<dyn FaultAdapter>>) -> Self {
        Self {
            adapters: Arc::new(adapters),
            active: Arc::new(RwLock::new(HashMap::new())),
            journal: Arc::new(FaultJournal::new()),
            metrics: Arc::new(FaultMetrics::default()),
        }
    }

    /// Create with a single noop adapter (for testing).
    pub fn noop() -> Self {
        Self::new(vec![Box::new(NoopAdapter::new())])
    }

    pub fn metrics(&self) -> &Arc<FaultMetrics> {
        &self.metrics
    }

    pub fn journal(&self) -> &Arc<FaultJournal> {
        &self.journal
    }

    /// Find an adapter that supports the given fault type.
    fn find_adapter(&self, fault_type: &FaultType) -> Option<usize> {
        self.adapters
            .as_ref()
            .iter()
            .position(|a| a.supported_faults().contains(fault_type))
    }

    /// Inject a fault via the appropriate adapter.
    pub async fn inject(&self, spec: FaultSpec) -> Result<FaultHandle, String> {
        let adapter_idx = self
            .find_adapter(&spec.fault_type)
            .ok_or_else(|| format!("no adapter for fault type {:?}", spec.fault_type))?;

        let handle = self.adapters[adapter_idx].inject(spec.clone()).await?;

        self.journal.record(JournalEntry {
            timestamp_ms: handle.injected_at_ms,
            action: JournalAction::Injected,
            fault_id: handle.id,
            fault_type: spec.fault_type.clone(),
            target: spec.target.clone(),
        });

        self.metrics
            .gashammer_faults_injected
            .fetch_add(1, Ordering::Relaxed);

        self.active
            .write()
            .await
            .insert(handle.id, (adapter_idx, handle.clone()));

        Ok(handle)
    }

    /// Clear a specific fault.
    pub async fn clear(&self, handle_id: Uuid) -> Result<(), String> {
        let (adapter_idx, handle) = {
            let active = self.active.read().await;
            active
                .get(&handle_id)
                .cloned()
                .ok_or_else(|| format!("fault {handle_id} not active"))?
        };

        self.adapters[adapter_idx].clear(handle_id).await?;

        self.active.write().await.remove(&handle_id);

        self.journal.record(JournalEntry {
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            action: JournalAction::Cleared,
            fault_id: handle_id,
            fault_type: handle.spec.fault_type,
            target: handle.spec.target,
        });

        self.metrics
            .gashammer_faults_reverted
            .fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    /// Clear all active faults (safety: called on shutdown).
    pub async fn clear_all(&self) -> Result<u32, String> {
        let active = self.active.read().await;
        let ids: Vec<(Uuid, usize, FaultHandle)> = active
            .iter()
            .map(|(id, (idx, h))| (*id, *idx, h.clone()))
            .collect();
        drop(active);

        let mut cleared = 0u32;
        for (id, adapter_idx, handle) in ids {
            if self.adapters[adapter_idx].clear(id).await.is_ok() {
                self.journal.record(JournalEntry {
                    timestamp_ms: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64,
                    action: JournalAction::EmergencyCleared,
                    fault_id: id,
                    fault_type: handle.spec.fault_type,
                    target: handle.spec.target,
                });
                cleared += 1;
            }
        }

        self.active.write().await.clear();

        self.metrics
            .gashammer_faults_reverted
            .fetch_add(cleared as u64, Ordering::Relaxed);

        Ok(cleared)
    }

    /// Number of active faults.
    pub async fn active_count(&self) -> usize {
        self.active.read().await.len()
    }

    /// Start auto-clear timer for a fault with a duration.
    ///
    /// When the timer fires, the adapter's `clear()` method is called
    /// to actually revert the fault (not just remove from the active map).
    pub fn start_auto_clear(&self, handle_id: Uuid, duration: Duration) -> JoinHandle<()> {
        let active = Arc::clone(&self.active);
        let adapters = Arc::clone(&self.adapters);
        let journal = Arc::clone(&self.journal);
        let metrics = Arc::clone(&self.metrics);

        tokio::spawn(async move {
            tokio::time::sleep(duration).await;

            let entry = {
                let mut active_map = active.write().await;
                active_map.remove(&handle_id)
            };

            if let Some((adapter_idx, handle)) = entry {
                // Actually clear the fault via the adapter
                if let Err(e) = adapters[adapter_idx].clear(handle_id).await {
                    tracing::warn!(
                        fault_id = %handle_id,
                        error = %e,
                        "auto-clear failed for fault"
                    );
                }

                journal.record(JournalEntry {
                    timestamp_ms: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64,
                    action: JournalAction::AutoCleared,
                    fault_id: handle_id,
                    fault_type: handle.spec.fault_type,
                    target: handle.spec.target,
                });
                metrics
                    .gashammer_faults_reverted
                    .fetch_add(1, Ordering::Relaxed);
                tracing::info!(fault_id = %handle_id, "fault auto-cleared after timeout");
            }
        })
    }

    /// Run preflight checks on all adapters.
    pub async fn preflight_all(&self) -> Vec<PreflightResult> {
        let mut results = Vec::new();
        for adapter in self.adapters.as_ref() {
            results.push(adapter.preflight_check().await);
        }
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_spec(fault_type: FaultType) -> FaultSpec {
        FaultSpec {
            fault_type,
            target: "127.0.0.1".into(),
            parameters: HashMap::new(),
            duration: None,
        }
    }

    // ── NoopAdapter ──

    #[tokio::test]
    async fn test_noop_adapter_preflight() {
        let adapter = NoopAdapter::new();
        let result = adapter.preflight_check().await;
        assert!(result.ready);
        assert!(result.issues.is_empty());
    }

    #[tokio::test]
    async fn test_noop_adapter_inject_and_clear() {
        let adapter = NoopAdapter::new();
        let handle = adapter
            .inject(test_spec(FaultType::NetworkLatency))
            .await
            .unwrap();
        assert!(!handle.cleared);

        adapter.clear(handle.id).await.unwrap();
    }

    #[tokio::test]
    async fn test_noop_adapter_clear_unknown() {
        let adapter = NoopAdapter::new();
        let result = adapter.clear(Uuid::new_v4()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_noop_adapter_clear_all() {
        let adapter = NoopAdapter::new();
        adapter
            .inject(test_spec(FaultType::NetworkLatency))
            .await
            .unwrap();
        adapter
            .inject(test_spec(FaultType::PacketLoss))
            .await
            .unwrap();

        let cleared = adapter.clear_all().await.unwrap();
        assert_eq!(cleared, 2);
    }

    #[tokio::test]
    async fn test_noop_adapter_supported_faults() {
        let adapter = NoopAdapter::new();
        let faults = adapter.supported_faults();
        assert!(faults.contains(&FaultType::NetworkLatency));
        assert!(faults.contains(&FaultType::PacketLoss));
        assert!(faults.contains(&FaultType::FeedDisconnect));
    }

    // ── FaultJournal ──

    #[test]
    fn test_journal_record_and_read() {
        let journal = FaultJournal::new();
        assert!(journal.is_empty());

        journal.record(JournalEntry {
            timestamp_ms: 1000,
            action: JournalAction::Injected,
            fault_id: Uuid::nil(),
            fault_type: FaultType::NetworkLatency,
            target: "127.0.0.1".into(),
        });

        assert_eq!(journal.len(), 1);
        let entries = journal.entries();
        assert_eq!(entries[0].target, "127.0.0.1");
    }

    // ── FaultManager ──

    #[tokio::test]
    async fn test_manager_inject_and_clear() {
        let manager = FaultManager::noop();

        let handle = manager
            .inject(test_spec(FaultType::NetworkLatency))
            .await
            .unwrap();
        assert_eq!(manager.active_count().await, 1);
        assert_eq!(manager.metrics().injected(), 1);

        manager.clear(handle.id).await.unwrap();
        assert_eq!(manager.active_count().await, 0);
        assert_eq!(manager.metrics().reverted(), 1);
    }

    #[tokio::test]
    async fn test_manager_clear_all() {
        let manager = FaultManager::noop();

        manager
            .inject(test_spec(FaultType::NetworkLatency))
            .await
            .unwrap();
        manager
            .inject(test_spec(FaultType::PacketLoss))
            .await
            .unwrap();
        assert_eq!(manager.active_count().await, 2);

        let cleared = manager.clear_all().await.unwrap();
        assert_eq!(cleared, 2);
        assert_eq!(manager.active_count().await, 0);
    }

    #[tokio::test]
    async fn test_manager_journal_tracking() {
        let manager = FaultManager::noop();

        let handle = manager
            .inject(test_spec(FaultType::NetworkLatency))
            .await
            .unwrap();
        manager.clear(handle.id).await.unwrap();

        let entries = manager.journal().entries();
        assert_eq!(entries.len(), 2);
        assert!(matches!(entries[0].action, JournalAction::Injected));
        assert!(matches!(entries[1].action, JournalAction::Cleared));
    }

    #[tokio::test]
    async fn test_manager_preflight() {
        let manager = FaultManager::noop();
        let results = manager.preflight_all().await;
        assert_eq!(results.len(), 1);
        assert!(results[0].ready);
    }

    #[tokio::test]
    async fn test_manager_auto_clear() {
        let manager = FaultManager::noop();

        let handle = manager
            .inject(test_spec(FaultType::NetworkLatency))
            .await
            .unwrap();
        assert_eq!(manager.active_count().await, 1);

        let timer = manager.start_auto_clear(handle.id, Duration::from_millis(50));

        tokio::time::sleep(Duration::from_millis(100)).await;

        assert_eq!(manager.active_count().await, 0);
        assert_eq!(manager.journal().len(), 2); // inject + auto-clear

        timer.abort();
    }

    #[tokio::test]
    async fn test_manager_clear_unknown_fails() {
        let manager = FaultManager::noop();
        let result = manager.clear(Uuid::new_v4()).await;
        assert!(result.is_err());
    }

    // ── Serialization ──

    #[test]
    fn test_fault_spec_serialization() {
        let spec = test_spec(FaultType::NetworkLatency);
        let json = serde_json::to_string(&spec).expect("serialize");
        let parsed: FaultSpec = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.fault_type, FaultType::NetworkLatency);
    }

    #[test]
    fn test_fault_handle_serialization() {
        let handle = FaultHandle {
            id: Uuid::nil(),
            spec: test_spec(FaultType::PacketLoss),
            injected_at_ms: 1000,
            cleared: false,
        };
        let json = serde_json::to_string(&handle).expect("serialize");
        let parsed: FaultHandle = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.spec.fault_type, FaultType::PacketLoss);
    }

    #[test]
    fn test_journal_entry_serialization() {
        let entry = JournalEntry {
            timestamp_ms: 1000,
            action: JournalAction::Injected,
            fault_id: Uuid::nil(),
            fault_type: FaultType::NetworkLatency,
            target: "127.0.0.1".into(),
        };
        let json = serde_json::to_string(&entry).expect("serialize");
        let _parsed: JournalEntry = serde_json::from_str(&json).expect("deserialize");
    }
}
