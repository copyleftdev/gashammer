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

//! Scenario compiler — validate, resolve, and compile into execution plan.
//!
//! Pipeline: Parse → Validate → Resolve → Compile.
//!
//! Ref: RFC-0010 §4–5.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::parser::ScenarioDefinition;

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// A validation error with a field path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}

/// Built-in template names that don't need custom definitions.
const BUILTIN_TEMPLATES: &[&str] = &[
    "simple-transfer",
    "noop",
    "erc20-transfer",
    "erc20-approve",
    "storage-write",
    "storage-write-heavy",
    "compute-heavy",
    "calldata-heavy",
];

/// Validate a parsed scenario definition.
pub fn validate(def: &ScenarioDefinition) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    // Version check
    if def.version != "1" {
        errors.push(ValidationError {
            field: "version".into(),
            message: format!("unsupported version '{}', expected '1'", def.version),
        });
    }

    // Scenario name
    if def.scenario.name.is_empty() {
        errors.push(ValidationError {
            field: "scenario.name".into(),
            message: "scenario name must not be empty".into(),
        });
    }

    // Target
    if def.target.sequencer_url.is_empty() {
        errors.push(ValidationError {
            field: "target.sequencer_url".into(),
            message: "sequencer_url must not be empty".into(),
        });
    }

    // Phases
    if def.phases.is_empty() {
        errors.push(ValidationError {
            field: "phases".into(),
            message: "at least one phase is required".into(),
        });
    }

    for (i, phase) in def.phases.iter().enumerate() {
        if phase.name.is_empty() {
            errors.push(ValidationError {
                field: format!("phases[{i}].name"),
                message: "phase name must not be empty".into(),
            });
        }
        if phase.duration_sec == 0 {
            errors.push(ValidationError {
                field: format!("phases[{i}].duration_sec"),
                message: "phase duration must be > 0".into(),
            });
        }
    }

    // Workload templates
    if def.workload.templates.is_empty() {
        errors.push(ValidationError {
            field: "workload.templates".into(),
            message: "at least one template is required".into(),
        });
    }

    for (name, weight) in &def.workload.templates {
        if !BUILTIN_TEMPLATES.contains(&name.as_str()) {
            errors.push(ValidationError {
                field: format!("workload.templates.{name}"),
                message: format!("unknown template '{name}' (not a built-in template)"),
            });
        }
        if *weight <= 0.0 {
            errors.push(ValidationError {
                field: format!("workload.templates.{name}"),
                message: "template weight must be > 0".into(),
            });
        }
    }

    // Regional allocation
    if !def.edges.regions.is_empty() {
        let total: u32 = def.edges.regions.values().sum();
        if total != 100 {
            errors.push(ValidationError {
                field: "edges.regions".into(),
                message: format!("regional allocation must sum to 100, got {total}"),
            });
        }
    }

    // Fault timing
    if let Some(faults) = &def.faults {
        let total_duration_sec: u64 = def.phases.iter().map(|p| p.duration_sec).sum();
        for (i, entry) in faults.schedule.iter().enumerate() {
            if entry.offset_sec > total_duration_sec {
                errors.push(ValidationError {
                    field: format!("faults.schedule[{i}].offset_sec"),
                    message: format!(
                        "fault offset {}s exceeds total duration {}s",
                        entry.offset_sec, total_duration_sec
                    ),
                });
            }
        }
    }

    errors
}

// ---------------------------------------------------------------------------
// Compiled scenario
// ---------------------------------------------------------------------------

/// A compiled phase with resolved configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledPhase {
    pub name: String,
    pub duration_sec: u64,
    pub gas_per_sec: u64,
    pub template_weights: Vec<(String, f64)>,
}

/// A fully compiled scenario ready for execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledScenario {
    /// Unique scenario ID.
    pub scenario_id: Uuid,
    /// Scenario name.
    pub name: String,
    /// PRNG seed.
    pub seed: u64,
    /// Target chain config.
    pub sequencer_url: String,
    pub gateway_url: Option<String>,
    pub feed_url: Option<String>,
    pub environment: String,
    /// Compiled phases.
    pub phases: Vec<CompiledPhase>,
    /// Total duration in seconds.
    pub total_duration_sec: u64,
    /// Edge requirements.
    pub min_edges: u32,
    pub account_count: u32,
    /// SHA-256 hash of the source YAML (DNA provenance).
    pub source_hash: String,
    /// Compiler version (DNA).
    pub compiler_version: String,
}

// ---------------------------------------------------------------------------
// Compile function
// ---------------------------------------------------------------------------

/// Compile a validated scenario definition into an execution plan.
///
/// This function does NOT validate — call `validate()` first.
pub fn compile(def: &ScenarioDefinition, source_yaml: &str) -> CompiledScenario {
    // Compute source hash for DNA provenance
    let source_hash = simple_hash(source_yaml);

    // Default gas rate from workload config
    let default_gas_per_sec = match &def.workload.gas_rate {
        crate::parser::GasRateConfig::Sustained { gas_per_sec } => *gas_per_sec,
        crate::parser::GasRateConfig::Ramped {
            end_gas_per_sec, ..
        } => *end_gas_per_sec,
        crate::parser::GasRateConfig::Bursty {
            baseline_gas_per_sec,
            ..
        } => *baseline_gas_per_sec,
    };

    let default_weights: Vec<(String, f64)> = def
        .workload
        .templates
        .iter()
        .map(|(k, v)| (k.clone(), *v))
        .collect();

    let phases: Vec<CompiledPhase> = def
        .phases
        .iter()
        .map(|p| {
            let gas_per_sec = p
                .gas_rate
                .as_ref()
                .map(|gr| match gr {
                    crate::parser::GasRateConfig::Sustained { gas_per_sec } => *gas_per_sec,
                    crate::parser::GasRateConfig::Ramped {
                        end_gas_per_sec, ..
                    } => *end_gas_per_sec,
                    crate::parser::GasRateConfig::Bursty {
                        baseline_gas_per_sec,
                        ..
                    } => *baseline_gas_per_sec,
                })
                .unwrap_or(default_gas_per_sec);

            let weights = p
                .template_weights
                .as_ref()
                .map(|tw| tw.iter().map(|(k, v)| (k.clone(), *v)).collect())
                .unwrap_or_else(|| default_weights.clone());

            CompiledPhase {
                name: p.name.clone(),
                duration_sec: p.duration_sec,
                gas_per_sec,
                template_weights: weights,
            }
        })
        .collect();

    let total_duration_sec: u64 = phases.iter().map(|p| p.duration_sec).sum();

    CompiledScenario {
        scenario_id: Uuid::new_v4(),
        name: def.scenario.name.clone(),
        seed: def.scenario.seed,
        sequencer_url: def.target.sequencer_url.clone(),
        gateway_url: def.target.gateway_url.clone(),
        feed_url: def.target.feed_url.clone(),
        environment: def.target.environment.clone(),
        phases,
        total_duration_sec,
        min_edges: def.edges.min_edges,
        account_count: def.edges.account_count,
        source_hash,
        compiler_version: format!("gashammer-scenario {}", env!("CARGO_PKG_VERSION")),
    }
}

/// Simple hash for source provenance (not cryptographic).
fn simple_hash(input: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325; // FNV-1a offset basis
    for byte in input.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3); // FNV prime
    }
    format!("{hash:016x}")
}

/// Full pipeline: parse → validate → compile.
pub fn parse_validate_compile(yaml: &str) -> Result<CompiledScenario, Vec<ValidationError>> {
    let def = crate::parser::parse_yaml(yaml).map_err(|e| {
        vec![ValidationError {
            field: "<yaml>".into(),
            message: e,
        }]
    })?;

    let errors = validate(&def);
    if !errors.is_empty() {
        return Err(errors);
    }

    Ok(compile(&def, yaml))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_yaml;

    const VALID_YAML: &str = r#"
version: "1"
scenario:
  name: test
  seed: 42
target:
  sequencer_url: "http://localhost:8547"
phases:
  - name: main
    duration_sec: 60
workload:
  gas_rate:
    mode: sustained
    gas_per_sec: 1000000
  templates:
    simple-transfer: 0.7
    noop: 0.3
"#;

    #[test]
    fn test_validate_valid_scenario() {
        let def = parse_yaml(VALID_YAML).unwrap();
        let errors = validate(&def);
        assert!(errors.is_empty(), "expected no errors, got: {errors:?}");
    }

    #[test]
    fn test_validate_bad_version() {
        let yaml = VALID_YAML.replace("version: \"1\"", "version: \"99\"");
        let def = parse_yaml(&yaml).unwrap();
        let errors = validate(&def);
        assert!(errors.iter().any(|e| e.field == "version"));
    }

    #[test]
    fn test_validate_empty_name() {
        let yaml = VALID_YAML.replace("name: test", "name: \"\"");
        let def = parse_yaml(&yaml).unwrap();
        let errors = validate(&def);
        assert!(errors.iter().any(|e| e.field == "scenario.name"));
    }

    #[test]
    fn test_validate_unknown_template() {
        let yaml = VALID_YAML.replace("simple-transfer: 0.7", "unknown-template: 0.7");
        let def = parse_yaml(&yaml).unwrap();
        let errors = validate(&def);
        assert!(errors
            .iter()
            .any(|e| e.message.contains("unknown template")));
    }

    #[test]
    fn test_validate_zero_duration_phase() {
        let yaml = VALID_YAML.replace("duration_sec: 60", "duration_sec: 0");
        let def = parse_yaml(&yaml).unwrap();
        let errors = validate(&def);
        assert!(errors
            .iter()
            .any(|e| e.message.contains("duration must be > 0")));
    }

    #[test]
    fn test_validate_region_sum_not_100() {
        let yaml = r#"
version: "1"
scenario:
  name: test
target:
  sequencer_url: "http://localhost:8547"
phases:
  - name: main
    duration_sec: 60
edges:
  regions:
    us-east-1: 60
    eu-west-1: 30
workload:
  gas_rate:
    mode: sustained
    gas_per_sec: 1000000
  templates:
    simple-transfer: 1.0
"#;
        let def = parse_yaml(yaml).unwrap();
        let errors = validate(&def);
        assert!(errors.iter().any(|e| e.message.contains("sum to 100")));
    }

    #[test]
    fn test_validate_fault_beyond_duration() {
        let yaml = r#"
version: "1"
scenario:
  name: test
target:
  sequencer_url: "http://localhost:8547"
phases:
  - name: main
    duration_sec: 60
workload:
  gas_rate:
    mode: sustained
    gas_per_sec: 1000000
  templates:
    simple-transfer: 1.0
faults:
  schedule:
    - offset_sec: 999
      action: inject
      fault_name: latency
"#;
        let def = parse_yaml(yaml).unwrap();
        let errors = validate(&def);
        assert!(errors.iter().any(|e| e.message.contains("exceeds total")));
    }

    #[test]
    fn test_compile_produces_valid_output() {
        let def = parse_yaml(VALID_YAML).unwrap();
        let compiled = compile(&def, VALID_YAML);

        assert_eq!(compiled.name, "test");
        assert_eq!(compiled.seed, 42);
        assert_eq!(compiled.phases.len(), 1);
        assert_eq!(compiled.total_duration_sec, 60);
        assert_eq!(compiled.min_edges, 1);
        assert_eq!(compiled.account_count, 100);
        assert!(!compiled.source_hash.is_empty());
        assert!(compiled.compiler_version.contains("gashammer-scenario"));
    }

    #[test]
    fn test_compile_phase_inherits_defaults() {
        let def = parse_yaml(VALID_YAML).unwrap();
        let compiled = compile(&def, VALID_YAML);

        assert_eq!(compiled.phases[0].gas_per_sec, 1_000_000);
        assert_eq!(compiled.phases[0].template_weights.len(), 2);
    }

    #[test]
    fn test_parse_validate_compile_pipeline() {
        let result = parse_validate_compile(VALID_YAML);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_validate_compile_invalid() {
        let yaml = VALID_YAML.replace("version: \"1\"", "version: \"99\"");
        let result = parse_validate_compile(&yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_source_hash_deterministic() {
        let h1 = simple_hash("hello");
        let h2 = simple_hash("hello");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_source_hash_different_input() {
        let h1 = simple_hash("hello");
        let h2 = simple_hash("world");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_compiled_scenario_serialization() {
        let compiled = parse_validate_compile(VALID_YAML).unwrap();
        let json = serde_json::to_string(&compiled).expect("serialize");
        let parsed: CompiledScenario = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.name, "test");
    }

    // -----------------------------------------------------------------------
    // Example scenario validation tests
    // -----------------------------------------------------------------------

    fn validate_scenario_file(yaml: &str, expected_name: &str) {
        let compiled = parse_validate_compile(yaml)
            .unwrap_or_else(|errs| panic!("validation failed for {expected_name}: {errs:?}"));
        assert_eq!(compiled.name, expected_name);
        assert!(!compiled.phases.is_empty());
        assert!(compiled.total_duration_sec > 0);
    }

    #[test]
    fn test_example_simple_transfer_load() {
        let yaml = include_str!("../../../scenarios/simple-transfer-load.yaml");
        validate_scenario_file(yaml, "simple-transfer-load");
    }

    #[test]
    fn test_example_mixed_workload() {
        let yaml = include_str!("../../../scenarios/mixed-workload.yaml");
        validate_scenario_file(yaml, "mixed-workload");
    }

    #[test]
    fn test_example_ramp_to_saturation() {
        let yaml = include_str!("../../../scenarios/ramp-to-saturation.yaml");
        validate_scenario_file(yaml, "ramp-to-saturation");
    }

    #[test]
    fn test_example_fault_tolerance() {
        let yaml = include_str!("../../../scenarios/fault-tolerance.yaml");
        validate_scenario_file(yaml, "fault-tolerance");
    }

    #[test]
    fn test_example_correctness_audit() {
        let yaml = include_str!("../../../scenarios/correctness-audit.yaml");
        validate_scenario_file(yaml, "correctness-audit");
    }

    #[test]
    fn test_example_quick_smoke_test() {
        let yaml = include_str!("../../../scenarios/quick-smoke-test.yaml");
        validate_scenario_file(yaml, "quick-smoke-test");
    }
}
