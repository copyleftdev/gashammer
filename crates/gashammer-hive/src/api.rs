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

//! REST API for the hive control plane.
//!
//! All routes are prefixed with `/api/v1`. Responses include the
//! `X-Powered-By: GasHammer/<version>` header (DNA provenance).
//!
//! Ref: RFC-0003 §6.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use gashammer_common::build_info::{GASHAMMER_NAME, GASHAMMER_VERSION};
use gashammer_common::{EdgeId, RunId, ScenarioId};

use crate::registry::{CapacityClass, EdgeRecord, EdgeRegistry, EdgeStatus, InstanceProfile};
use crate::run::{Phase, RunConfig, RunOrchestrator, RunState};

// ---------------------------------------------------------------------------
// API configuration
// ---------------------------------------------------------------------------

/// Configuration for the REST API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    /// Bearer token for API authentication.
    pub api_key: String,
    /// Listen address (default: "0.0.0.0:8080").
    pub listen_addr: String,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            listen_addr: "0.0.0.0:8080".into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Shared application state
// ---------------------------------------------------------------------------

/// A stored scenario record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioRecord {
    pub scenario_id: ScenarioId,
    pub name: String,
    pub description: String,
    pub definition: serde_json::Value,
}

/// Shared state available to all API handlers.
pub struct AppState {
    pub registry: EdgeRegistry,
    pub orchestrator: RunOrchestrator,
    pub scenarios: RwLock<HashMap<ScenarioId, ScenarioRecord>>,
    pub api_key: String,
}

impl AppState {
    pub fn new(registry: EdgeRegistry, orchestrator: RunOrchestrator, api_key: String) -> Self {
        Self {
            registry,
            orchestrator,
            scenarios: RwLock::new(HashMap::new()),
            api_key,
        }
    }
}

// ---------------------------------------------------------------------------
// Error response format (RFC-0003 §6.2)
// ---------------------------------------------------------------------------

/// Standard API error response per RFC-0003 §6.2.
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiError {
    pub error: ApiErrorBody,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiErrorBody {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl ApiError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error: ApiErrorBody {
                code: code.into(),
                message: message.into(),
                details: None,
            },
        }
    }

    pub fn with_details(
        code: impl Into<String>,
        message: impl Into<String>,
        details: serde_json::Value,
    ) -> Self {
        Self {
            error: ApiErrorBody {
                code: code.into(),
                message: message.into(),
                details: Some(details),
            },
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        // Determine status code from error code prefix
        let status = match self.error.code.as_str() {
            "NOT_FOUND" | "EDGE_NOT_FOUND" | "RUN_NOT_FOUND" | "SCENARIO_NOT_FOUND" => {
                StatusCode::NOT_FOUND
            }
            "UNAUTHORIZED" => StatusCode::UNAUTHORIZED,
            "VALIDATION_FAILED" | "INVALID_REQUEST" => StatusCode::BAD_REQUEST,
            "CONFLICT" => StatusCode::CONFLICT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };

        (status, Json(self)).into_response()
    }
}

// ---------------------------------------------------------------------------
// Middleware: X-Powered-By header (DNA provenance)
// ---------------------------------------------------------------------------

/// Middleware that adds `X-Powered-By: GasHammer/<version>` to every response.
pub async fn powered_by_header(
    request: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Response {
    let mut response = next.run(request).await;
    let value = format!("{GASHAMMER_NAME}/{GASHAMMER_VERSION}");
    if let Ok(hv) = HeaderValue::from_str(&value) {
        response.headers_mut().insert("x-powered-by", hv);
    }
    response
}

// ---------------------------------------------------------------------------
// Middleware: Bearer token authentication (RFC-0003 §6.3)
// ---------------------------------------------------------------------------

/// Middleware that validates `Authorization: Bearer <key>`.
pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    request: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Response {
    // Skip auth if no API key is configured (dev mode)
    if state.api_key.is_empty() {
        return next.run(request).await;
    }

    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    match auth_header {
        Some(value) if value.starts_with("Bearer ") => {
            let token = &value[7..];
            if token == state.api_key {
                next.run(request).await
            } else {
                ApiError::new("UNAUTHORIZED", "invalid API key").into_response()
            }
        }
        _ => ApiError::new("UNAUTHORIZED", "missing or malformed Authorization header")
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// Router construction
// ---------------------------------------------------------------------------

/// Build the complete API router with all routes and middleware.
pub fn router(state: Arc<AppState>) -> Router {
    let api = Router::new()
        // Edge routes
        .route("/edges", get(list_edges))
        .route("/edges/:id", get(get_edge))
        .route("/edges/:id/drain", post(drain_edge))
        // Scenario routes (stubs for now — will be implemented with scenario crate)
        .route("/scenarios", get(list_scenarios).post(create_scenario))
        .route("/scenarios/:id", get(get_scenario))
        // Run routes (stubs for now — will be implemented with run orchestrator)
        .route("/runs", get(list_runs).post(create_run))
        .route("/runs/:id", get(get_run))
        .route("/runs/:id/abort", post(abort_run))
        .route("/runs/:id/report", get(get_run_report))
        // System routes
        .route("/health", get(health_check))
        .route("/version", get(version_info))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .with_state(state);

    Router::new()
        .nest("/api/v1", api)
        .layer(middleware::from_fn(powered_by_header))
}

// ---------------------------------------------------------------------------
// Edge handlers
// ---------------------------------------------------------------------------

/// Response type for edge listings.
#[derive(Debug, Serialize, Deserialize)]
struct EdgeResponse {
    edge_id: String,
    region: String,
    capacity_class: CapacityClass,
    software_version: String,
    supported_adapters: Vec<String>,
    instance_profile: InstanceProfile,
    status: EdgeStatus,
}

impl From<EdgeRecord> for EdgeResponse {
    fn from(r: EdgeRecord) -> Self {
        Self {
            edge_id: r.edge_id.to_string(),
            region: r.region,
            capacity_class: r.capacity_class,
            software_version: r.software_version,
            supported_adapters: r.supported_adapters,
            instance_profile: r.instance_profile,
            status: r.status,
        }
    }
}

/// GET /api/v1/edges — list all registered edges.
async fn list_edges(State(state): State<Arc<AppState>>) -> Json<Vec<EdgeResponse>> {
    let edges = state.registry.list().await;
    Json(edges.into_iter().map(EdgeResponse::from).collect())
}

/// GET /api/v1/edges/:id — get edge details.
async fn get_edge(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<EdgeResponse>, ApiError> {
    let edge_id: EdgeId = id
        .parse()
        .map_err(|_| ApiError::new("INVALID_REQUEST", "invalid edge ID format"))?;

    state
        .registry
        .get(&edge_id)
        .await
        .map(|r| Json(EdgeResponse::from(r)))
        .ok_or_else(|| ApiError::new("EDGE_NOT_FOUND", format!("edge {edge_id} not found")))
}

/// POST /api/v1/edges/:id/drain — drain an edge.
async fn drain_edge(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let edge_id: EdgeId = id
        .parse()
        .map_err(|_| ApiError::new("INVALID_REQUEST", "invalid edge ID format"))?;

    state
        .registry
        .drain(&edge_id)
        .await
        .map_err(|e| ApiError::new("CONFLICT", e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Scenario handlers
// ---------------------------------------------------------------------------

/// Response type for scenario listings.
#[derive(Debug, Serialize, Deserialize)]
struct ScenarioResponse {
    scenario_id: String,
    name: String,
    description: String,
}

impl From<ScenarioRecord> for ScenarioResponse {
    fn from(r: ScenarioRecord) -> Self {
        Self {
            scenario_id: r.scenario_id.to_string(),
            name: r.name,
            description: r.description,
        }
    }
}

/// Request body for creating a scenario.
#[derive(Debug, Deserialize)]
struct CreateScenarioRequest {
    name: String,
    #[serde(default)]
    description: String,
    definition: serde_json::Value,
}

/// GET /api/v1/scenarios — list all scenarios.
async fn list_scenarios(State(state): State<Arc<AppState>>) -> Json<Vec<ScenarioResponse>> {
    let scenarios = state.scenarios.read().await;
    Json(
        scenarios
            .values()
            .cloned()
            .map(ScenarioResponse::from)
            .collect(),
    )
}

/// POST /api/v1/scenarios — create a scenario.
async fn create_scenario(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateScenarioRequest>,
) -> Result<(StatusCode, Json<ScenarioResponse>), ApiError> {
    let scenario_id = ScenarioId::new();
    let record = ScenarioRecord {
        scenario_id: scenario_id.clone(),
        name: body.name,
        description: body.description,
        definition: body.definition,
    };

    let response = ScenarioResponse::from(record.clone());
    state.scenarios.write().await.insert(scenario_id, record);

    Ok((StatusCode::CREATED, Json(response)))
}

/// GET /api/v1/scenarios/:id — get scenario details.
async fn get_scenario(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ScenarioRecord>, ApiError> {
    let scenario_id: ScenarioId = id
        .parse()
        .map_err(|_| ApiError::new("INVALID_REQUEST", "invalid scenario ID format"))?;

    let scenarios = state.scenarios.read().await;
    scenarios
        .get(&scenario_id)
        .cloned()
        .map(Json)
        .ok_or_else(|| {
            ApiError::new(
                "SCENARIO_NOT_FOUND",
                format!("scenario {scenario_id} not found"),
            )
        })
}

// ---------------------------------------------------------------------------
// Run handlers
// ---------------------------------------------------------------------------

/// Request body for creating a run.
#[derive(Debug, Deserialize)]
struct CreateRunRequest {
    scenario_id: String,
    #[serde(default)]
    seed: Option<u64>,
    #[serde(default)]
    edge_ids: Option<Vec<String>>,
}

/// Response type for a run.
#[derive(Debug, Serialize, Deserialize)]
struct RunResponse {
    run_id: String,
    scenario_id: String,
    state: RunState,
    current_phase: usize,
    timeline_event_count: usize,
}

/// Response type for run listings.
#[derive(Debug, Serialize, Deserialize)]
struct RunListItem {
    run_id: String,
    state: RunState,
}

/// Request body for aborting a run.
#[derive(Debug, Deserialize)]
struct AbortRunRequest {
    #[serde(default = "default_abort_reason")]
    reason: String,
}

fn default_abort_reason() -> String {
    "aborted via API".into()
}

/// Response type for a run report.
#[derive(Debug, Serialize, Deserialize)]
struct RunReportResponse {
    run_id: String,
    state: RunState,
    current_phase: usize,
    timeline: Vec<RunEventResponse>,
}

/// A single timeline event in a report.
#[derive(Debug, Serialize, Deserialize)]
struct RunEventResponse {
    timestamp_ms: u64,
    event_type: String,
    description: String,
}

/// GET /api/v1/runs — list all runs.
async fn list_runs(State(state): State<Arc<AppState>>) -> Json<Vec<RunListItem>> {
    let runs = state.orchestrator.list_runs().await;
    Json(
        runs.into_iter()
            .map(|(id, st)| RunListItem {
                run_id: id.to_string(),
                state: st,
            })
            .collect(),
    )
}

/// POST /api/v1/runs — create and start a run.
async fn create_run(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateRunRequest>,
) -> Result<(StatusCode, Json<RunResponse>), ApiError> {
    let scenario_id: ScenarioId = body
        .scenario_id
        .parse()
        .map_err(|_| ApiError::new("INVALID_REQUEST", "invalid scenario ID format"))?;

    // Verify scenario exists
    let scenarios = state.scenarios.read().await;
    if !scenarios.contains_key(&scenario_id) {
        return Err(ApiError::new(
            "SCENARIO_NOT_FOUND",
            format!("scenario {scenario_id} not found"),
        ));
    }
    drop(scenarios);

    // Resolve edge IDs: use provided list or pick all available
    let edge_ids = match body.edge_ids {
        Some(ids) => {
            let mut parsed = Vec::with_capacity(ids.len());
            for id_str in &ids {
                let eid: EdgeId = id_str
                    .parse()
                    .map_err(|_| ApiError::new("INVALID_REQUEST", "invalid edge ID format"))?;
                parsed.push(eid);
            }
            parsed
        }
        None => {
            let available = state.registry.list_available().await;
            available.into_iter().map(|r| r.edge_id).collect()
        }
    };

    let config = RunConfig {
        scenario_id: scenario_id.clone(),
        seed: body.seed.unwrap_or(42),
        phases: vec![Phase {
            name: "default".into(),
            duration: Duration::from_secs(60),
            target_gas_rate: 1_000_000,
        }],
        edge_ids,
        barrier_timeout: Duration::from_secs(30),
        max_clock_skew: Duration::from_millis(250),
    };

    let run_id = state
        .orchestrator
        .create_run(config)
        .await
        .map_err(|e| ApiError::new("CONFLICT", e.to_string()))?;

    let record =
        state.orchestrator.get_run(&run_id).await.ok_or_else(|| {
            ApiError::new("NOT_FOUND", "run was created but could not be retrieved")
        })?;

    Ok((
        StatusCode::CREATED,
        Json(RunResponse {
            run_id: run_id.to_string(),
            scenario_id: scenario_id.to_string(),
            state: record.state,
            current_phase: record.current_phase,
            timeline_event_count: record.timeline.len(),
        }),
    ))
}

/// GET /api/v1/runs/:id — get run status.
async fn get_run(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<RunResponse>, ApiError> {
    let run_id: RunId = id
        .parse()
        .map_err(|_| ApiError::new("INVALID_REQUEST", "invalid run ID format"))?;

    let record = state
        .orchestrator
        .get_run(&run_id)
        .await
        .ok_or_else(|| ApiError::new("RUN_NOT_FOUND", format!("run {run_id} not found")))?;

    Ok(Json(RunResponse {
        run_id: run_id.to_string(),
        scenario_id: record.config.scenario_id.to_string(),
        state: record.state,
        current_phase: record.current_phase,
        timeline_event_count: record.timeline.len(),
    }))
}

/// POST /api/v1/runs/:id/abort — abort a run.
async fn abort_run(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    body: Option<Json<AbortRunRequest>>,
) -> Result<StatusCode, ApiError> {
    let run_id: RunId = id
        .parse()
        .map_err(|_| ApiError::new("INVALID_REQUEST", "invalid run ID format"))?;

    let reason = body
        .map(|b| b.0.reason)
        .unwrap_or_else(default_abort_reason);

    state
        .orchestrator
        .abort_run(&run_id, reason)
        .await
        .map_err(|e| ApiError::new("CONFLICT", e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/v1/runs/:id/report — get run report with timeline.
async fn get_run_report(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<RunReportResponse>, ApiError> {
    let run_id: RunId = id
        .parse()
        .map_err(|_| ApiError::new("INVALID_REQUEST", "invalid run ID format"))?;

    let record = state
        .orchestrator
        .get_run(&run_id)
        .await
        .ok_or_else(|| ApiError::new("RUN_NOT_FOUND", format!("run {run_id} not found")))?;

    let timeline = record
        .timeline
        .iter()
        .map(|e| RunEventResponse {
            timestamp_ms: e.timestamp.as_millis() as u64,
            event_type: format!("{:?}", e.event_type),
            description: e.description.clone(),
        })
        .collect();

    Ok(Json(RunReportResponse {
        run_id: run_id.to_string(),
        state: record.state,
        current_phase: record.current_phase,
        timeline,
    }))
}

// ---------------------------------------------------------------------------
// System handlers
// ---------------------------------------------------------------------------

/// Health check response.
#[derive(Debug, Serialize, Deserialize)]
struct HealthResponse {
    status: String,
    edges_registered: u64,
    edges_active: u64,
}

/// GET /api/v1/health — hive health check.
async fn health_check(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    let metrics = state.registry.metrics();
    Json(HealthResponse {
        status: "ok".into(),
        edges_registered: metrics.registered_count(),
        edges_active: metrics.active_count(),
    })
}

/// GET /api/v1/version — version information.
async fn version_info() -> Json<gashammer_common::BuildInfo> {
    Json(gashammer_common::build_info())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;
    use uuid::Uuid;

    use crate::registry::{HeartbeatPayload, RegisterEdgeRequest, RegistryConfig};
    use crate::run::RunOrchestrator;

    fn test_state() -> Arc<AppState> {
        Arc::new(AppState::new(
            EdgeRegistry::new(RegistryConfig::default()),
            RunOrchestrator::new(),
            String::new(), // no auth in tests
        ))
    }

    fn test_state_with_auth(key: &str) -> Arc<AppState> {
        Arc::new(AppState::new(
            EdgeRegistry::new(RegistryConfig::default()),
            RunOrchestrator::new(),
            key.to_string(),
        ))
    }

    async fn register_test_edge(state: &Arc<AppState>) -> EdgeId {
        let edge_id = state
            .registry
            .register(RegisterEdgeRequest {
                bootstrap_token: "test".into(),
                region: "us-west-2".into(),
                capacity_class: CapacityClass::Small,
                software_version: "0.1.0".into(),
                supported_adapters: vec!["nitro-rpc".into()],
                instance_profile: InstanceProfile {
                    cpu_cores: 4,
                    memory_mb: 8192,
                    network_class: "standard".into(),
                },
            })
            .await
            .expect("register should succeed");

        // Send heartbeat to make it Healthy
        state
            .registry
            .heartbeat(
                &edge_id,
                HeartbeatPayload {
                    cpu_utilization: 0.5,
                    memory_utilization: 0.3,
                    active_connections: 1,
                    current_run_id: None,
                },
            )
            .await
            .expect("heartbeat should succeed");

        edge_id
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let state = test_state();
        let app = router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let health: HealthResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(health.status, "ok");
    }

    #[tokio::test]
    async fn test_version_endpoint() {
        let state = test_state();
        let app = router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/version")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["name"], "GasHammer");
        assert_eq!(json["license"], "BSL-1.1");
    }

    #[tokio::test]
    async fn test_powered_by_header_present() {
        let state = test_state();
        let app = router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let powered_by = response
            .headers()
            .get("x-powered-by")
            .expect("x-powered-by header should exist");
        let value = powered_by.to_str().unwrap();
        assert!(value.starts_with("GasHammer/"));
    }

    #[tokio::test]
    async fn test_list_edges_empty() {
        let state = test_state();
        let app = router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/edges")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let edges: Vec<EdgeResponse> = serde_json::from_slice(&body).unwrap();
        assert!(edges.is_empty());
    }

    #[tokio::test]
    async fn test_list_edges_with_registered_edge() {
        let state = test_state();
        let _edge_id = register_test_edge(&state).await;
        let app = router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/edges")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let edges: Vec<EdgeResponse> = serde_json::from_slice(&body).unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].region, "us-west-2");
    }

    #[tokio::test]
    async fn test_get_edge_found() {
        let state = test_state();
        let edge_id = register_test_edge(&state).await;
        let app = router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/edges/{edge_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let edge: EdgeResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(edge.edge_id, edge_id.to_string());
    }

    #[tokio::test]
    async fn test_get_edge_not_found() {
        let state = test_state();
        let app = router(state);

        let fake_id = Uuid::new_v4();
        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/edges/{fake_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_edge_invalid_id() {
        let state = test_state();
        let app = router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/edges/not-a-uuid")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_drain_edge() {
        let state = test_state();
        let edge_id = register_test_edge(&state).await;
        let app = router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/edges/{edge_id}/drain"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn test_auth_required_when_configured() {
        let state = test_state_with_auth("secret-key");
        let app = router(state);

        // No auth header
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_auth_valid_token() {
        let state = test_state_with_auth("secret-key");
        let app = router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/health")
                    .header("Authorization", "Bearer secret-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_auth_invalid_token() {
        let state = test_state_with_auth("secret-key");
        let app = router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/health")
                    .header("Authorization", "Bearer wrong-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_error_format() {
        let state = test_state();
        let app = router(state);

        let fake_id = Uuid::new_v4();
        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/edges/{fake_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let err: ApiError = serde_json::from_slice(&body).unwrap();
        assert_eq!(err.error.code, "EDGE_NOT_FOUND");
        assert!(err.error.message.contains("not found"));
    }

    #[tokio::test]
    async fn test_scenarios_stub_returns_empty() {
        let state = test_state();
        let app = router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/scenarios")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_runs_returns_empty() {
        let state = test_state();
        let app = router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/runs")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let runs: Vec<RunListItem> = serde_json::from_slice(&body).unwrap();
        assert!(runs.is_empty());
    }

    #[tokio::test]
    async fn test_create_and_get_scenario() {
        let state = test_state();
        let app = router(state.clone());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/scenarios")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&serde_json::json!({
                            "name": "test-scenario",
                            "description": "A test scenario",
                            "definition": {"version": "1"}
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let scenario: ScenarioResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(scenario.name, "test-scenario");

        // Now get it
        let app = router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/scenarios/{}", scenario.scenario_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let record: ScenarioRecord = serde_json::from_slice(&body).unwrap();
        assert_eq!(record.name, "test-scenario");
    }

    #[tokio::test]
    async fn test_get_scenario_not_found() {
        let state = test_state();
        let app = router(state);
        let fake_id = Uuid::new_v4();

        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/scenarios/{fake_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    async fn create_test_scenario(state: &Arc<AppState>) -> ScenarioId {
        let scenario_id = ScenarioId::new();
        state.scenarios.write().await.insert(
            scenario_id.clone(),
            ScenarioRecord {
                scenario_id: scenario_id.clone(),
                name: "test".into(),
                description: "test scenario".into(),
                definition: serde_json::json!({"version": "1"}),
            },
        );
        scenario_id
    }

    #[tokio::test]
    async fn test_create_run_and_get_status() {
        let state = test_state();
        let scenario_id = create_test_scenario(&state).await;
        let app = router(state.clone());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/runs")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&serde_json::json!({
                            "scenario_id": scenario_id.to_string(),
                            "seed": 42
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let run: RunResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(run.scenario_id, scenario_id.to_string());
        assert_eq!(run.state, RunState::Preflight);

        // Get the run
        let app = router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/runs/{}", run.run_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_create_run_unknown_scenario_fails() {
        let state = test_state();
        let app = router(state);
        let fake_id = Uuid::new_v4();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/runs")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&serde_json::json!({
                            "scenario_id": fake_id.to_string()
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_abort_run() {
        let state = test_state();
        let scenario_id = create_test_scenario(&state).await;

        // Create a run
        let run_id = state
            .orchestrator
            .create_run(RunConfig {
                scenario_id,
                seed: 42,
                phases: vec![],
                edge_ids: vec![],
                ..RunConfig::default()
            })
            .await
            .unwrap();

        let app = router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/runs/{run_id}/abort"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&serde_json::json!({
                            "reason": "test abort"
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn test_get_run_not_found() {
        let state = test_state();
        let app = router(state);
        let fake_id = Uuid::new_v4();

        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/runs/{fake_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_run_report() {
        let state = test_state();
        let scenario_id = create_test_scenario(&state).await;

        let run_id = state
            .orchestrator
            .create_run(RunConfig {
                scenario_id,
                seed: 42,
                phases: vec![],
                edge_ids: vec![],
                ..RunConfig::default()
            })
            .await
            .unwrap();

        let app = router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/runs/{run_id}/report"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let report: RunReportResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(report.run_id, run_id.to_string());
        assert!(!report.timeline.is_empty()); // At least PreflightStarted
    }

    #[tokio::test]
    async fn test_list_runs_after_creation() {
        let state = test_state();
        let scenario_id = create_test_scenario(&state).await;

        state
            .orchestrator
            .create_run(RunConfig {
                scenario_id,
                seed: 42,
                phases: vec![],
                edge_ids: vec![],
                ..RunConfig::default()
            })
            .await
            .unwrap();

        let app = router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/runs")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let runs: Vec<RunListItem> = serde_json::from_slice(&body).unwrap();
        assert_eq!(runs.len(), 1);
    }
}
