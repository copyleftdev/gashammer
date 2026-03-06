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

//! Fault timeline — schedule faults at specific times within a run.
//!
//! The timeline is an ordered sequence of inject/clear events. The
//! executor advances through the timeline as the run progresses.
//!
//! Ref: RFC-0008 §4.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::adapter::{FaultManager, FaultSpec};

// ---------------------------------------------------------------------------
// Edge selector
// ---------------------------------------------------------------------------

/// Selector for which edges a fault applies to.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeSelector {
    /// All edges.
    All,
    /// Edges in a specific region.
    Region(String),
    /// Specific edges by ID.
    Specific(Vec<Uuid>),
}

// ---------------------------------------------------------------------------
// Timeline events
// ---------------------------------------------------------------------------

/// Action in a timeline event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimelineAction {
    /// Inject a fault.
    Inject(FaultSpec),
    /// Clear a previously injected fault by name.
    Clear { fault_name: String },
}

/// A scheduled fault event in the timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaultTimelineEvent {
    /// Offset from run start.
    pub offset_ms: u64,
    /// Human-readable name for this fault (used for clear references).
    pub fault_name: String,
    /// Which edges this applies to.
    pub target_edges: EdgeSelector,
    /// The action to take.
    pub action: TimelineAction,
}

// ---------------------------------------------------------------------------
// Timeline
// ---------------------------------------------------------------------------

/// An ordered sequence of fault events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaultTimeline {
    /// Events sorted by offset_ms.
    events: Vec<FaultTimelineEvent>,
    /// Environment allowlist (empty = all environments allowed).
    pub allowed_environments: Vec<String>,
    /// Blocked environments (e.g., "mainnet", "production").
    pub blocked_environments: Vec<String>,
}

impl FaultTimeline {
    /// Create a new timeline from events. Events are sorted by offset.
    pub fn new(mut events: Vec<FaultTimelineEvent>) -> Self {
        events.sort_by_key(|e| e.offset_ms);
        Self {
            events,
            allowed_environments: Vec::new(),
            blocked_environments: vec!["mainnet".into(), "production".into()],
        }
    }

    /// Create an empty timeline.
    pub fn empty() -> Self {
        Self::new(Vec::new())
    }

    /// Number of events.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Whether the timeline is empty.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// All events in order.
    pub fn events(&self) -> &[FaultTimelineEvent] {
        &self.events
    }

    /// Check if faults are allowed in the given environment.
    pub fn is_environment_allowed(&self, env: &str) -> bool {
        if self.blocked_environments.iter().any(|e| e == env) {
            return false;
        }
        if self.allowed_environments.is_empty() {
            return true;
        }
        self.allowed_environments.iter().any(|e| e == env)
    }

    /// Get events that should fire between `from_ms` (exclusive) and `to_ms` (inclusive).
    pub fn events_in_range(&self, from_ms: u64, to_ms: u64) -> Vec<&FaultTimelineEvent> {
        self.events
            .iter()
            .filter(|e| e.offset_ms > from_ms && e.offset_ms <= to_ms)
            .collect()
    }

    /// Total duration of the timeline (offset of last event).
    pub fn duration_ms(&self) -> u64 {
        self.events.last().map(|e| e.offset_ms).unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// Executor
// ---------------------------------------------------------------------------

/// Execution mode for the timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionMode {
    /// Execute faults for real.
    Live,
    /// Log but don't execute (dry run).
    DryRun,
}

/// Record of an executed timeline event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRecord {
    pub offset_ms: u64,
    pub fault_name: String,
    pub action: String,
    pub success: bool,
    pub error: Option<String>,
}

/// Executes a fault timeline by advancing through events as time progresses.
pub struct TimelineExecutor {
    timeline: FaultTimeline,
    mode: ExecutionMode,
    cursor_ms: u64,
    /// Map of fault_name → active handle ID.
    active_handles: HashMap<String, Uuid>,
    /// Execution log.
    log: Vec<ExecutionRecord>,
}

impl TimelineExecutor {
    /// Create a new executor.
    pub fn new(timeline: FaultTimeline, mode: ExecutionMode) -> Self {
        Self {
            timeline,
            mode,
            cursor_ms: 0,
            active_handles: HashMap::new(),
            log: Vec::new(),
        }
    }

    /// Advance the executor to the given elapsed time.
    /// Executes all events between the previous cursor and `elapsed_ms`.
    pub async fn advance(
        &mut self,
        elapsed_ms: u64,
        manager: &FaultManager,
    ) -> Vec<ExecutionRecord> {
        let events = self
            .timeline
            .events_in_range(self.cursor_ms, elapsed_ms)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();

        self.cursor_ms = elapsed_ms;
        let mut records = Vec::new();

        for event in events {
            let record = match &event.action {
                TimelineAction::Inject(spec) => self.execute_inject(&event, spec, manager).await,
                TimelineAction::Clear { fault_name } => {
                    self.execute_clear(&event, fault_name, manager).await
                }
            };
            self.log.push(record.clone());
            records.push(record);
        }

        records
    }

    async fn execute_inject(
        &mut self,
        event: &FaultTimelineEvent,
        spec: &FaultSpec,
        manager: &FaultManager,
    ) -> ExecutionRecord {
        if self.mode == ExecutionMode::DryRun {
            tracing::info!(
                fault_name = %event.fault_name,
                offset_ms = event.offset_ms,
                "dry-run: would inject fault"
            );
            self.active_handles
                .insert(event.fault_name.clone(), Uuid::nil());
            return ExecutionRecord {
                offset_ms: event.offset_ms,
                fault_name: event.fault_name.clone(),
                action: "inject (dry-run)".into(),
                success: true,
                error: None,
            };
        }

        match manager.inject(spec.clone()).await {
            Ok(handle) => {
                self.active_handles
                    .insert(event.fault_name.clone(), handle.id);
                ExecutionRecord {
                    offset_ms: event.offset_ms,
                    fault_name: event.fault_name.clone(),
                    action: "inject".into(),
                    success: true,
                    error: None,
                }
            }
            Err(err) => ExecutionRecord {
                offset_ms: event.offset_ms,
                fault_name: event.fault_name.clone(),
                action: "inject".into(),
                success: false,
                error: Some(err),
            },
        }
    }

    async fn execute_clear(
        &mut self,
        event: &FaultTimelineEvent,
        fault_name: &str,
        manager: &FaultManager,
    ) -> ExecutionRecord {
        if self.mode == ExecutionMode::DryRun {
            tracing::info!(
                fault_name = %fault_name,
                offset_ms = event.offset_ms,
                "dry-run: would clear fault"
            );
            self.active_handles.remove(fault_name);
            return ExecutionRecord {
                offset_ms: event.offset_ms,
                fault_name: event.fault_name.clone(),
                action: "clear (dry-run)".into(),
                success: true,
                error: None,
            };
        }

        if let Some(handle_id) = self.active_handles.remove(fault_name) {
            match manager.clear(handle_id).await {
                Ok(()) => ExecutionRecord {
                    offset_ms: event.offset_ms,
                    fault_name: event.fault_name.clone(),
                    action: "clear".into(),
                    success: true,
                    error: None,
                },
                Err(err) => ExecutionRecord {
                    offset_ms: event.offset_ms,
                    fault_name: event.fault_name.clone(),
                    action: "clear".into(),
                    success: false,
                    error: Some(err),
                },
            }
        } else {
            ExecutionRecord {
                offset_ms: event.offset_ms,
                fault_name: event.fault_name.clone(),
                action: "clear".into(),
                success: false,
                error: Some(format!("no active fault named '{fault_name}'")),
            }
        }
    }

    /// Clear all remaining active faults (safety: called at run end).
    pub async fn clear_all_remaining(&mut self, manager: &FaultManager) -> u32 {
        if self.mode == ExecutionMode::DryRun {
            let count = self.active_handles.len() as u32;
            self.active_handles.clear();
            return count;
        }

        let handles: Vec<(String, Uuid)> = self.active_handles.drain().collect();
        let mut cleared = 0u32;
        for (name, handle_id) in handles {
            if manager.clear(handle_id).await.is_ok() {
                self.log.push(ExecutionRecord {
                    offset_ms: self.cursor_ms,
                    fault_name: name,
                    action: "clear (run-end)".into(),
                    success: true,
                    error: None,
                });
                cleared += 1;
            }
        }
        cleared
    }

    /// Number of currently active faults.
    pub fn active_count(&self) -> usize {
        self.active_handles.len()
    }

    /// Execution log.
    pub fn execution_log(&self) -> &[ExecutionRecord] {
        &self.log
    }

    /// The timeline being executed.
    pub fn timeline(&self) -> &FaultTimeline {
        &self.timeline
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::{FaultManager, FaultSpec, FaultType};
    use std::collections::HashMap;

    fn latency_spec() -> FaultSpec {
        FaultSpec {
            fault_type: FaultType::NetworkLatency,
            target: "127.0.0.1".into(),
            parameters: {
                let mut p = HashMap::new();
                p.insert("delay_ms".into(), "100".into());
                p
            },
            duration: None,
        }
    }

    fn simple_timeline() -> FaultTimeline {
        FaultTimeline::new(vec![
            FaultTimelineEvent {
                offset_ms: 1000,
                fault_name: "latency".into(),
                target_edges: EdgeSelector::All,
                action: TimelineAction::Inject(latency_spec()),
            },
            FaultTimelineEvent {
                offset_ms: 5000,
                fault_name: "latency".into(),
                target_edges: EdgeSelector::All,
                action: TimelineAction::Clear {
                    fault_name: "latency".into(),
                },
            },
        ])
    }

    // ── FaultTimeline ──

    #[test]
    fn test_timeline_sorting() {
        let timeline = FaultTimeline::new(vec![
            FaultTimelineEvent {
                offset_ms: 5000,
                fault_name: "b".into(),
                target_edges: EdgeSelector::All,
                action: TimelineAction::Clear {
                    fault_name: "a".into(),
                },
            },
            FaultTimelineEvent {
                offset_ms: 1000,
                fault_name: "a".into(),
                target_edges: EdgeSelector::All,
                action: TimelineAction::Inject(latency_spec()),
            },
        ]);

        assert_eq!(timeline.events()[0].offset_ms, 1000);
        assert_eq!(timeline.events()[1].offset_ms, 5000);
    }

    #[test]
    fn test_timeline_events_in_range() {
        let timeline = simple_timeline();

        let events = timeline.events_in_range(0, 1000);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].fault_name, "latency");

        let events = timeline.events_in_range(1000, 5000);
        assert_eq!(events.len(), 1);

        let events = timeline.events_in_range(0, 10000);
        assert_eq!(events.len(), 2);

        let events = timeline.events_in_range(5000, 10000);
        assert!(events.is_empty());
    }

    #[test]
    fn test_timeline_duration() {
        let timeline = simple_timeline();
        assert_eq!(timeline.duration_ms(), 5000);
    }

    #[test]
    fn test_timeline_empty() {
        let timeline = FaultTimeline::empty();
        assert!(timeline.is_empty());
        assert_eq!(timeline.duration_ms(), 0);
    }

    #[test]
    fn test_environment_blocked() {
        let timeline = simple_timeline();
        assert!(!timeline.is_environment_allowed("mainnet"));
        assert!(!timeline.is_environment_allowed("production"));
        assert!(timeline.is_environment_allowed("devnet"));
        assert!(timeline.is_environment_allowed("testnet"));
    }

    // ── TimelineExecutor ──

    #[tokio::test]
    async fn test_executor_advance_injects_and_clears() {
        let timeline = simple_timeline();
        let manager = FaultManager::noop();
        let mut executor = TimelineExecutor::new(timeline, ExecutionMode::Live);

        // Advance to 1s — should inject
        let records = executor.advance(1000, &manager).await;
        assert_eq!(records.len(), 1);
        assert!(records[0].success);
        assert_eq!(records[0].action, "inject");
        assert_eq!(executor.active_count(), 1);

        // Advance to 5s — should clear
        let records = executor.advance(5000, &manager).await;
        assert_eq!(records.len(), 1);
        assert!(records[0].success);
        assert_eq!(records[0].action, "clear");
        assert_eq!(executor.active_count(), 0);
    }

    #[tokio::test]
    async fn test_executor_dry_run() {
        let timeline = simple_timeline();
        let manager = FaultManager::noop();
        let mut executor = TimelineExecutor::new(timeline, ExecutionMode::DryRun);

        let records = executor.advance(1000, &manager).await;
        assert_eq!(records.len(), 1);
        assert!(records[0].action.contains("dry-run"));
        assert_eq!(executor.active_count(), 1);

        let records = executor.advance(5000, &manager).await;
        assert_eq!(records.len(), 1);
        assert!(records[0].action.contains("dry-run"));
        assert_eq!(executor.active_count(), 0);
    }

    #[tokio::test]
    async fn test_executor_overlapping_faults() {
        let timeline = FaultTimeline::new(vec![
            FaultTimelineEvent {
                offset_ms: 1000,
                fault_name: "latency".into(),
                target_edges: EdgeSelector::All,
                action: TimelineAction::Inject(latency_spec()),
            },
            FaultTimelineEvent {
                offset_ms: 2000,
                fault_name: "packet_loss".into(),
                target_edges: EdgeSelector::All,
                action: TimelineAction::Inject(FaultSpec {
                    fault_type: FaultType::PacketLoss,
                    target: "127.0.0.1".into(),
                    parameters: HashMap::new(),
                    duration: None,
                }),
            },
            FaultTimelineEvent {
                offset_ms: 3000,
                fault_name: "latency".into(),
                target_edges: EdgeSelector::All,
                action: TimelineAction::Clear {
                    fault_name: "latency".into(),
                },
            },
            FaultTimelineEvent {
                offset_ms: 4000,
                fault_name: "packet_loss".into(),
                target_edges: EdgeSelector::All,
                action: TimelineAction::Clear {
                    fault_name: "packet_loss".into(),
                },
            },
        ]);

        let manager = FaultManager::noop();
        let mut executor = TimelineExecutor::new(timeline, ExecutionMode::Live);

        executor.advance(2000, &manager).await;
        assert_eq!(executor.active_count(), 2);

        executor.advance(3000, &manager).await;
        assert_eq!(executor.active_count(), 1);

        executor.advance(4000, &manager).await;
        assert_eq!(executor.active_count(), 0);
    }

    #[tokio::test]
    async fn test_executor_clear_all_remaining() {
        let timeline = FaultTimeline::new(vec![FaultTimelineEvent {
            offset_ms: 1000,
            fault_name: "latency".into(),
            target_edges: EdgeSelector::All,
            action: TimelineAction::Inject(latency_spec()),
        }]);

        let manager = FaultManager::noop();
        let mut executor = TimelineExecutor::new(timeline, ExecutionMode::Live);

        executor.advance(1000, &manager).await;
        assert_eq!(executor.active_count(), 1);

        let cleared = executor.clear_all_remaining(&manager).await;
        assert_eq!(cleared, 1);
        assert_eq!(executor.active_count(), 0);
    }

    #[tokio::test]
    async fn test_executor_execution_log() {
        let timeline = simple_timeline();
        let manager = FaultManager::noop();
        let mut executor = TimelineExecutor::new(timeline, ExecutionMode::Live);

        executor.advance(5000, &manager).await;
        assert_eq!(executor.execution_log().len(), 2);
    }

    #[tokio::test]
    async fn test_executor_clear_nonexistent_fault() {
        let timeline = FaultTimeline::new(vec![FaultTimelineEvent {
            offset_ms: 1000,
            fault_name: "ghost".into(),
            target_edges: EdgeSelector::All,
            action: TimelineAction::Clear {
                fault_name: "ghost".into(),
            },
        }]);

        let manager = FaultManager::noop();
        let mut executor = TimelineExecutor::new(timeline, ExecutionMode::Live);

        let records = executor.advance(1000, &manager).await;
        assert_eq!(records.len(), 1);
        assert!(!records[0].success);
    }

    // ── Serialization ──

    #[test]
    fn test_timeline_serialization() {
        let timeline = simple_timeline();
        let json = serde_json::to_string(&timeline).expect("serialize");
        let parsed: FaultTimeline = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.len(), 2);
    }

    #[test]
    fn test_edge_selector_serialization() {
        let selectors = vec![
            EdgeSelector::All,
            EdgeSelector::Region("us-east-1".into()),
            EdgeSelector::Specific(vec![Uuid::nil()]),
        ];
        for sel in &selectors {
            let json = serde_json::to_string(sel).expect("serialize");
            let _parsed: EdgeSelector = serde_json::from_str(&json).expect("deserialize");
        }
    }
}
