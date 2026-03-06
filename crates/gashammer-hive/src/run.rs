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

//! Run orchestration — state machine, barrier sync, phase progression.
//!
//! Ref: RFC-0003 §5.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::time::Instant;

use gashammer_common::{EdgeId, RunId, ScenarioId};

use crate::error::HiveError;

// ---------------------------------------------------------------------------
// Run state machine (RFC-0003 §5.1)
// ---------------------------------------------------------------------------

/// Run lifecycle state per RFC-0003 §5.1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunState {
    /// Health checks, skew estimation, credential validation.
    Preflight,
    /// Waiting for all edges to acknowledge readiness.
    Barrier,
    /// Workload is active.
    Running {
        phase_index: usize,
        phase_name: String,
    },
    /// Post-workload: waiting for drain + oracle checks.
    Recovering,
    /// Generating reports, storing artifacts.
    Completing,
    /// Terminal: run finished.
    Done { outcome: RunOutcome },
    /// Terminal: run was aborted.
    Aborted { reason: String },
}

impl RunState {
    /// Returns `true` if this is a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self, RunState::Done { .. } | RunState::Aborted { .. })
    }

    /// Returns `true` if this state is active (workload running or recovering).
    pub fn is_active(&self) -> bool {
        matches!(self, RunState::Running { .. } | RunState::Recovering)
    }

    /// Validate a state transition.
    pub(crate) fn can_transition_to(&self, target: &RunState) -> bool {
        match (self, target) {
            (RunState::Preflight, RunState::Barrier) => true,
            (RunState::Preflight, RunState::Aborted { .. }) => true,
            (RunState::Barrier, RunState::Running { .. }) => true,
            (RunState::Barrier, RunState::Aborted { .. }) => true,
            (RunState::Running { .. }, RunState::Running { .. }) => true, // phase transition
            (RunState::Running { .. }, RunState::Recovering) => true,
            (RunState::Running { .. }, RunState::Aborted { .. }) => true,
            (RunState::Recovering, RunState::Completing) => true,
            (RunState::Recovering, RunState::Aborted { .. }) => true,
            (RunState::Completing, RunState::Done { .. }) => true,
            (RunState::Completing, RunState::Aborted { .. }) => true,
            _ => false,
        }
    }
}

/// Outcome of a completed run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunOutcome {
    Pass,
    Fail { violations: Vec<String> },
    Inconclusive { reason: String },
}

// ---------------------------------------------------------------------------
// Phase definitions
// ---------------------------------------------------------------------------

/// A phase within a run — defines workload parameters for a time segment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Phase {
    pub name: String,
    pub duration: Duration,
    pub target_gas_rate: u64,
}

// ---------------------------------------------------------------------------
// Run configuration and record
// ---------------------------------------------------------------------------

/// Configuration for a run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunConfig {
    pub scenario_id: ScenarioId,
    pub seed: u64,
    pub phases: Vec<Phase>,
    pub edge_ids: Vec<EdgeId>,
    /// Maximum time to wait for all edges at the barrier (default: 30s).
    pub barrier_timeout: Duration,
    /// Maximum clock skew allowed between edges (default: 250ms).
    pub max_clock_skew: Duration,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            scenario_id: ScenarioId::new(),
            seed: 0,
            phases: vec![],
            edge_ids: vec![],
            barrier_timeout: Duration::from_secs(30),
            max_clock_skew: Duration::from_millis(250),
        }
    }
}

/// A timeline event recorded during a run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunEvent {
    pub timestamp: Duration,
    pub event_type: RunEventType,
    pub description: String,
}

/// Types of events recorded in the run timeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunEventType {
    PreflightStarted,
    PreflightPassed,
    PreflightFailed,
    BarrierWaiting,
    BarrierReady,
    BarrierTimeout,
    PhaseStarted,
    PhaseCompleted,
    RunStarted,
    RunCompleted,
    RunAborted,
    EdgeLost,
}

/// Record of a run, persisted in memory (MVP) or PostgreSQL (future).
#[derive(Debug, Clone)]
pub struct RunRecord {
    pub run_id: RunId,
    pub config: RunConfig,
    pub state: RunState,
    pub started_at: Option<Instant>,
    pub completed_at: Option<Instant>,
    pub barrier_ready_edges: Vec<EdgeId>,
    pub timeline: Vec<RunEvent>,
    pub current_phase: usize,
}

impl RunRecord {
    fn new(run_id: RunId, config: RunConfig) -> Self {
        Self {
            run_id,
            config,
            state: RunState::Preflight,
            started_at: None,
            completed_at: None,
            barrier_ready_edges: vec![],
            timeline: vec![],
            current_phase: 0,
        }
    }

    fn elapsed(&self) -> Duration {
        self.started_at
            .map(|s| s.elapsed())
            .unwrap_or(Duration::ZERO)
    }

    fn add_event(&mut self, event_type: RunEventType, description: impl Into<String>) {
        self.timeline.push(RunEvent {
            timestamp: self.elapsed(),
            event_type,
            description: description.into(),
        });
    }
}

// ---------------------------------------------------------------------------
// Run orchestrator
// ---------------------------------------------------------------------------

/// Manages the lifecycle of all runs.
pub struct RunOrchestrator {
    runs: Arc<RwLock<HashMap<RunId, RunRecord>>>,
}

impl RunOrchestrator {
    pub fn new() -> Self {
        Self {
            runs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new run in Preflight state.
    #[tracing::instrument(skip(self, config))]
    pub async fn create_run(&self, config: RunConfig) -> Result<RunId, HiveError> {
        let run_id = RunId::new();
        let mut record = RunRecord::new(run_id.clone(), config);
        record.add_event(RunEventType::PreflightStarted, "run created");

        let mut runs = self.runs.write().await;
        runs.insert(run_id.clone(), record);

        tracing::info!(%run_id, "run created in Preflight state");
        Ok(run_id)
    }

    /// Mark preflight as passed and transition to Barrier state.
    #[tracing::instrument(skip(self), fields(%run_id))]
    pub async fn preflight_passed(&self, run_id: &RunId) -> Result<(), HiveError> {
        let mut runs = self.runs.write().await;
        let record = self.get_record_mut(&mut runs, run_id)?;

        if !record.state.can_transition_to(&RunState::Barrier) {
            return Err(HiveError::Orchestration(format!(
                "cannot transition from {:?} to Barrier",
                record.state
            )));
        }

        record.state = RunState::Barrier;
        record.add_event(RunEventType::PreflightPassed, "preflight checks passed");
        record.add_event(RunEventType::BarrierWaiting, "waiting for edges at barrier");

        tracing::info!(%run_id, "preflight passed, entering barrier");
        Ok(())
    }

    /// Mark preflight as failed and abort the run.
    #[tracing::instrument(skip(self), fields(%run_id))]
    pub async fn preflight_failed(&self, run_id: &RunId, reason: String) -> Result<(), HiveError> {
        let mut runs = self.runs.write().await;
        let record = self.get_record_mut(&mut runs, run_id)?;

        record.state = RunState::Aborted {
            reason: reason.clone(),
        };
        record.add_event(
            RunEventType::PreflightFailed,
            format!("preflight failed: {reason}"),
        );
        record.completed_at = Some(Instant::now());

        tracing::warn!(%run_id, %reason, "preflight failed, run aborted");
        Ok(())
    }

    /// Record that an edge is ready at the barrier.
    /// If all edges are ready, returns `true`.
    #[tracing::instrument(skip(self), fields(%run_id, %edge_id))]
    pub async fn barrier_edge_ready(
        &self,
        run_id: &RunId,
        edge_id: &EdgeId,
    ) -> Result<bool, HiveError> {
        let mut runs = self.runs.write().await;
        let record = self.get_record_mut(&mut runs, run_id)?;

        if record.state != RunState::Barrier {
            return Err(HiveError::Orchestration(format!(
                "run {} is not in Barrier state (current: {:?})",
                run_id, record.state
            )));
        }

        if !record.barrier_ready_edges.contains(edge_id) {
            record.barrier_ready_edges.push(edge_id.clone());
        }

        let all_ready = record.barrier_ready_edges.len() == record.config.edge_ids.len();
        if all_ready {
            record.add_event(RunEventType::BarrierReady, "all edges ready at barrier");
            tracing::info!(%run_id, "all edges ready at barrier");
        }

        Ok(all_ready)
    }

    /// Release the barrier and start the run. Transitions to Running(phase 0).
    #[tracing::instrument(skip(self), fields(%run_id))]
    pub async fn release_barrier(&self, run_id: &RunId) -> Result<(), HiveError> {
        let mut runs = self.runs.write().await;
        let record = self.get_record_mut(&mut runs, run_id)?;

        if record.state != RunState::Barrier {
            return Err(HiveError::Orchestration(format!(
                "run {} is not in Barrier state",
                run_id
            )));
        }

        let first_phase_name = record
            .config
            .phases
            .first()
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "default".into());

        record.state = RunState::Running {
            phase_index: 0,
            phase_name: first_phase_name.clone(),
        };
        record.started_at = Some(Instant::now());
        record.current_phase = 0;
        record.add_event(RunEventType::RunStarted, "workload started");
        record.add_event(
            RunEventType::PhaseStarted,
            format!("phase 0: {first_phase_name}"),
        );

        tracing::info!(%run_id, phase = first_phase_name, "run started");
        Ok(())
    }

    /// Advance to the next phase.
    #[tracing::instrument(skip(self), fields(%run_id))]
    pub async fn advance_phase(&self, run_id: &RunId) -> Result<bool, HiveError> {
        let mut runs = self.runs.write().await;
        let record = self.get_record_mut(&mut runs, run_id)?;

        if !matches!(record.state, RunState::Running { .. }) {
            return Err(HiveError::Orchestration(format!(
                "run {} is not Running",
                run_id
            )));
        }

        let next_index = record.current_phase + 1;

        if next_index >= record.config.phases.len() {
            // All phases complete → Recovering
            record.add_event(
                RunEventType::PhaseCompleted,
                format!("phase {} completed (last)", record.current_phase),
            );
            record.state = RunState::Recovering;
            tracing::info!(%run_id, "all phases complete, entering recovery");
            return Ok(false);
        }

        let phase_name = record.config.phases[next_index].name.clone();
        record.add_event(
            RunEventType::PhaseCompleted,
            format!("phase {} completed", record.current_phase),
        );
        record.state = RunState::Running {
            phase_index: next_index,
            phase_name: phase_name.clone(),
        };
        record.current_phase = next_index;
        record.add_event(
            RunEventType::PhaseStarted,
            format!("phase {next_index}: {phase_name}"),
        );

        tracing::info!(%run_id, phase_index = next_index, %phase_name, "phase advanced");
        Ok(true)
    }

    /// Transition from Recovering to Completing.
    #[tracing::instrument(skip(self), fields(%run_id))]
    pub async fn begin_completing(&self, run_id: &RunId) -> Result<(), HiveError> {
        let mut runs = self.runs.write().await;
        let record = self.get_record_mut(&mut runs, run_id)?;

        if record.state != RunState::Recovering {
            return Err(HiveError::Orchestration(format!(
                "run {} is not in Recovering state",
                run_id
            )));
        }

        record.state = RunState::Completing;
        tracing::info!(%run_id, "entering completing state");
        Ok(())
    }

    /// Complete the run with a final outcome.
    #[tracing::instrument(skip(self, outcome), fields(%run_id))]
    pub async fn complete_run(&self, run_id: &RunId, outcome: RunOutcome) -> Result<(), HiveError> {
        let mut runs = self.runs.write().await;
        let record = self.get_record_mut(&mut runs, run_id)?;

        if record.state != RunState::Completing {
            return Err(HiveError::Orchestration(format!(
                "run {} is not in Completing state",
                run_id
            )));
        }

        record.state = RunState::Done {
            outcome: outcome.clone(),
        };
        record.completed_at = Some(Instant::now());
        record.add_event(
            RunEventType::RunCompleted,
            format!("run completed: {outcome:?}"),
        );

        tracing::info!(%run_id, ?outcome, "run completed");
        Ok(())
    }

    /// Abort a run. Can be called from any non-terminal state.
    #[tracing::instrument(skip(self), fields(%run_id))]
    pub async fn abort_run(&self, run_id: &RunId, reason: String) -> Result<(), HiveError> {
        let mut runs = self.runs.write().await;
        let record = self.get_record_mut(&mut runs, run_id)?;

        if record.state.is_terminal() {
            return Err(HiveError::Orchestration(format!(
                "run {} is already in terminal state: {:?}",
                run_id, record.state
            )));
        }

        record.state = RunState::Aborted {
            reason: reason.clone(),
        };
        record.completed_at = Some(Instant::now());
        record.add_event(RunEventType::RunAborted, format!("aborted: {reason}"));

        tracing::warn!(%run_id, %reason, "run aborted");
        Ok(())
    }

    /// Get a snapshot of a run record.
    pub async fn get_run(&self, run_id: &RunId) -> Option<RunRecord> {
        let runs = self.runs.read().await;
        runs.get(run_id).cloned()
    }

    /// List all runs.
    pub async fn list_runs(&self) -> Vec<(RunId, RunState)> {
        let runs = self.runs.read().await;
        runs.iter()
            .map(|(id, r)| (id.clone(), r.state.clone()))
            .collect()
    }

    fn get_record_mut<'a>(
        &self,
        runs: &'a mut HashMap<RunId, RunRecord>,
        run_id: &RunId,
    ) -> Result<&'a mut RunRecord, HiveError> {
        runs.get_mut(run_id)
            .ok_or_else(|| HiveError::Orchestration(format!("unknown run: {run_id}")))
    }
}

impl Default for RunOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> RunConfig {
        RunConfig {
            scenario_id: ScenarioId::new(),
            seed: 42,
            phases: vec![
                Phase {
                    name: "ramp".into(),
                    duration: Duration::from_secs(30),
                    target_gas_rate: 1_000_000,
                },
                Phase {
                    name: "sustain".into(),
                    duration: Duration::from_secs(60),
                    target_gas_rate: 5_000_000,
                },
                Phase {
                    name: "recover".into(),
                    duration: Duration::from_secs(10),
                    target_gas_rate: 0,
                },
            ],
            edge_ids: vec![EdgeId::new(), EdgeId::new()],
            barrier_timeout: Duration::from_secs(30),
            max_clock_skew: Duration::from_millis(250),
        }
    }

    #[tokio::test]
    async fn test_create_run_starts_in_preflight() {
        let orch = RunOrchestrator::new();
        let run_id = orch.create_run(test_config()).await.unwrap();

        let record = orch.get_run(&run_id).await.unwrap();
        assert_eq!(record.state, RunState::Preflight);
        assert!(!record.timeline.is_empty());
    }

    #[tokio::test]
    async fn test_preflight_to_barrier() {
        let orch = RunOrchestrator::new();
        let run_id = orch.create_run(test_config()).await.unwrap();

        orch.preflight_passed(&run_id).await.unwrap();

        let record = orch.get_run(&run_id).await.unwrap();
        assert_eq!(record.state, RunState::Barrier);
    }

    #[tokio::test]
    async fn test_preflight_failed_aborts() {
        let orch = RunOrchestrator::new();
        let run_id = orch.create_run(test_config()).await.unwrap();

        orch.preflight_failed(&run_id, "clock skew too high".into())
            .await
            .unwrap();

        let record = orch.get_run(&run_id).await.unwrap();
        assert!(matches!(record.state, RunState::Aborted { .. }));
    }

    #[tokio::test]
    async fn test_barrier_edge_ready() {
        let orch = RunOrchestrator::new();
        let config = test_config();
        let edge_ids = config.edge_ids.clone();
        let run_id = orch.create_run(config).await.unwrap();

        orch.preflight_passed(&run_id).await.unwrap();

        // First edge ready — not all
        let all_ready = orch
            .barrier_edge_ready(&run_id, &edge_ids[0])
            .await
            .unwrap();
        assert!(!all_ready);

        // Second edge ready — all
        let all_ready = orch
            .barrier_edge_ready(&run_id, &edge_ids[1])
            .await
            .unwrap();
        assert!(all_ready);
    }

    #[tokio::test]
    async fn test_barrier_duplicate_edge_ready() {
        let orch = RunOrchestrator::new();
        let config = test_config();
        let edge_ids = config.edge_ids.clone();
        let run_id = orch.create_run(config).await.unwrap();

        orch.preflight_passed(&run_id).await.unwrap();

        orch.barrier_edge_ready(&run_id, &edge_ids[0])
            .await
            .unwrap();
        // Same edge again — should not double-count
        let all_ready = orch
            .barrier_edge_ready(&run_id, &edge_ids[0])
            .await
            .unwrap();
        assert!(!all_ready);
    }

    #[tokio::test]
    async fn test_release_barrier_starts_run() {
        let orch = RunOrchestrator::new();
        let config = test_config();
        let edge_ids = config.edge_ids.clone();
        let run_id = orch.create_run(config).await.unwrap();

        orch.preflight_passed(&run_id).await.unwrap();
        orch.barrier_edge_ready(&run_id, &edge_ids[0])
            .await
            .unwrap();
        orch.barrier_edge_ready(&run_id, &edge_ids[1])
            .await
            .unwrap();
        orch.release_barrier(&run_id).await.unwrap();

        let record = orch.get_run(&run_id).await.unwrap();
        assert!(matches!(
            record.state,
            RunState::Running { phase_index: 0, .. }
        ));
        assert!(record.started_at.is_some());
    }

    #[tokio::test]
    async fn test_phase_progression() {
        let orch = RunOrchestrator::new();
        let config = test_config();
        let edge_ids = config.edge_ids.clone();
        let run_id = orch.create_run(config).await.unwrap();

        orch.preflight_passed(&run_id).await.unwrap();
        for eid in &edge_ids {
            orch.barrier_edge_ready(&run_id, eid).await.unwrap();
        }
        orch.release_barrier(&run_id).await.unwrap();

        // Phase 0 → 1
        let more = orch.advance_phase(&run_id).await.unwrap();
        assert!(more);
        let record = orch.get_run(&run_id).await.unwrap();
        assert!(matches!(
            record.state,
            RunState::Running { phase_index: 1, .. }
        ));

        // Phase 1 → 2
        let more = orch.advance_phase(&run_id).await.unwrap();
        assert!(more);

        // Phase 2 → Recovering (last phase)
        let more = orch.advance_phase(&run_id).await.unwrap();
        assert!(!more);
        let record = orch.get_run(&run_id).await.unwrap();
        assert_eq!(record.state, RunState::Recovering);
    }

    #[tokio::test]
    async fn test_full_lifecycle_pass() {
        let orch = RunOrchestrator::new();
        let config = test_config();
        let edge_ids = config.edge_ids.clone();
        let run_id = orch.create_run(config).await.unwrap();

        // Preflight → Barrier → Running → Recovering → Completing → Done
        orch.preflight_passed(&run_id).await.unwrap();
        for eid in &edge_ids {
            orch.barrier_edge_ready(&run_id, eid).await.unwrap();
        }
        orch.release_barrier(&run_id).await.unwrap();

        // Advance through all phases
        while orch.advance_phase(&run_id).await.unwrap() {}

        orch.begin_completing(&run_id).await.unwrap();
        orch.complete_run(&run_id, RunOutcome::Pass).await.unwrap();

        let record = orch.get_run(&run_id).await.unwrap();
        assert_eq!(
            record.state,
            RunState::Done {
                outcome: RunOutcome::Pass
            }
        );
        assert!(record.completed_at.is_some());
    }

    #[tokio::test]
    async fn test_abort_from_running() {
        let orch = RunOrchestrator::new();
        let config = test_config();
        let edge_ids = config.edge_ids.clone();
        let run_id = orch.create_run(config).await.unwrap();

        orch.preflight_passed(&run_id).await.unwrap();
        for eid in &edge_ids {
            orch.barrier_edge_ready(&run_id, eid).await.unwrap();
        }
        orch.release_barrier(&run_id).await.unwrap();

        orch.abort_run(&run_id, "user requested abort".into())
            .await
            .unwrap();

        let record = orch.get_run(&run_id).await.unwrap();
        assert!(matches!(record.state, RunState::Aborted { .. }));
    }

    #[tokio::test]
    async fn test_abort_terminal_run_fails() {
        let orch = RunOrchestrator::new();
        let run_id = orch.create_run(test_config()).await.unwrap();

        orch.preflight_failed(&run_id, "test".into()).await.unwrap();

        let result = orch.abort_run(&run_id, "again".into()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_runs() {
        let orch = RunOrchestrator::new();
        let _id1 = orch.create_run(test_config()).await.unwrap();
        let _id2 = orch.create_run(test_config()).await.unwrap();

        let runs = orch.list_runs().await;
        assert_eq!(runs.len(), 2);
    }

    #[tokio::test]
    async fn test_get_unknown_run() {
        let orch = RunOrchestrator::new();
        let result = orch.get_run(&RunId::new()).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_timeline_events_recorded() {
        let orch = RunOrchestrator::new();
        let config = test_config();
        let edge_ids = config.edge_ids.clone();
        let run_id = orch.create_run(config).await.unwrap();

        orch.preflight_passed(&run_id).await.unwrap();
        for eid in &edge_ids {
            orch.barrier_edge_ready(&run_id, eid).await.unwrap();
        }
        orch.release_barrier(&run_id).await.unwrap();

        let record = orch.get_run(&run_id).await.unwrap();
        let event_types: Vec<_> = record.timeline.iter().map(|e| &e.event_type).collect();

        assert!(event_types.contains(&&RunEventType::PreflightStarted));
        assert!(event_types.contains(&&RunEventType::PreflightPassed));
        assert!(event_types.contains(&&RunEventType::BarrierWaiting));
        assert!(event_types.contains(&&RunEventType::BarrierReady));
        assert!(event_types.contains(&&RunEventType::RunStarted));
        assert!(event_types.contains(&&RunEventType::PhaseStarted));
    }

    // State machine transition tests
    #[test]
    fn test_state_preflight_transitions() {
        let s = RunState::Preflight;
        assert!(s.can_transition_to(&RunState::Barrier));
        assert!(s.can_transition_to(&RunState::Aborted {
            reason: "test".into()
        }));
        assert!(!s.can_transition_to(&RunState::Running {
            phase_index: 0,
            phase_name: "x".into()
        }));
    }

    #[test]
    fn test_state_barrier_transitions() {
        let s = RunState::Barrier;
        assert!(s.can_transition_to(&RunState::Running {
            phase_index: 0,
            phase_name: "x".into()
        }));
        assert!(s.can_transition_to(&RunState::Aborted {
            reason: "test".into()
        }));
        assert!(!s.can_transition_to(&RunState::Preflight));
    }

    #[test]
    fn test_state_running_transitions() {
        let s = RunState::Running {
            phase_index: 0,
            phase_name: "ramp".into(),
        };
        assert!(s.can_transition_to(&RunState::Running {
            phase_index: 1,
            phase_name: "sustain".into()
        }));
        assert!(s.can_transition_to(&RunState::Recovering));
        assert!(s.can_transition_to(&RunState::Aborted {
            reason: "test".into()
        }));
        assert!(!s.can_transition_to(&RunState::Preflight));
    }

    #[test]
    fn test_state_recovering_transitions() {
        let s = RunState::Recovering;
        assert!(s.can_transition_to(&RunState::Completing));
        assert!(s.can_transition_to(&RunState::Aborted {
            reason: "test".into()
        }));
        assert!(!s.can_transition_to(&RunState::Running {
            phase_index: 0,
            phase_name: "x".into()
        }));
    }

    #[test]
    fn test_state_done_is_terminal() {
        let s = RunState::Done {
            outcome: RunOutcome::Pass,
        };
        assert!(s.is_terminal());
        assert!(!s.can_transition_to(&RunState::Preflight));
    }

    #[test]
    fn test_state_aborted_is_terminal() {
        let s = RunState::Aborted {
            reason: "test".into(),
        };
        assert!(s.is_terminal());
    }

    #[test]
    fn test_run_outcome_serialization() {
        let outcomes = vec![
            RunOutcome::Pass,
            RunOutcome::Fail {
                violations: vec!["v1".into(), "v2".into()],
            },
            RunOutcome::Inconclusive {
                reason: "timeout".into(),
            },
        ];

        for outcome in &outcomes {
            let json = serde_json::to_string(outcome).expect("serialize");
            let parsed: RunOutcome = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(*outcome, parsed);
        }
    }
}
