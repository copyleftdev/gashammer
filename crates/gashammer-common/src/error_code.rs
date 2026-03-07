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

use std::fmt;

use serde::Serialize;

/// Error category corresponding to a GasHammer crate.
///
/// Each category owns a range of numeric error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum ErrorCategory {
    /// gashammer-common (E001–E099)
    Common,
    /// gashammer-nitro (E100–E199)
    Nitro,
    /// gashammer-edge (E200–E299)
    Edge,
    /// gashammer-hive (E300–E399)
    Hive,
    /// gashammer-workload (E400–E499)
    Workload,
    /// gashammer-telemetry (E500–E599)
    Telemetry,
    /// gashammer-oracle (E600–E699)
    Oracle,
    /// gashammer-fault (E700–E799)
    Fault,
    /// gashammer-report (E800–E899)
    Report,
    /// gashammer-scenario (E900–E949)
    Scenario,
    /// gashammer-docgen (E950–E979)
    Docgen,
    /// gashammer-testenv (E980–E999)
    Testenv,
}

impl fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Common => "common",
            Self::Nitro => "nitro",
            Self::Edge => "edge",
            Self::Hive => "hive",
            Self::Workload => "workload",
            Self::Telemetry => "telemetry",
            Self::Oracle => "oracle",
            Self::Fault => "fault",
            Self::Report => "report",
            Self::Scenario => "scenario",
            Self::Docgen => "docgen",
            Self::Testenv => "testenv",
        };
        f.write_str(name)
    }
}

/// Central registry of all GasHammer error codes.
///
/// The `GH-Exxx` prefix is part of GasHammer's structural DNA (RFC-0011 §8.3).
/// Each variant maps to a unique numeric code, a human-readable message,
/// and a category identifying the originating crate.
///
/// # Code Ranges
///
/// | Range     | Category   | Crate                |
/// |-----------|------------|----------------------|
/// | E001–E099 | Common     | gashammer-common     |
/// | E100–E199 | Nitro      | gashammer-nitro      |
/// | E200–E299 | Edge       | gashammer-edge       |
/// | E300–E399 | Hive       | gashammer-hive       |
/// | E400–E499 | Workload   | gashammer-workload   |
/// | E500–E599 | Telemetry  | gashammer-telemetry  |
/// | E600–E699 | Oracle     | gashammer-oracle     |
/// | E700–E799 | Fault      | gashammer-fault      |
/// | E800–E899 | Report     | gashammer-report     |
/// | E900–E949 | Scenario   | gashammer-scenario   |
/// | E950–E979 | Docgen     | gashammer-docgen     |
/// | E980–E999 | Testenv    | gashammer-testenv    |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum ErrorCode {
    // ── Common (E001–E099) ──────────────────────────────────────────
    /// GH-E001: Configuration loading or validation failed.
    ConfigError,
    /// GH-E002: Serialization or deserialization failed.
    SerializationError,
    /// GH-E003: An internal invariant was violated.
    InvariantViolation,

    // ── Nitro (E100–E199) ───────────────────────────────────────────
    /// GH-E100: JSON-RPC call failed.
    RpcError,
    /// GH-E101: Sequencer feed connection or parsing failed.
    FeedError,
    /// GH-E102: L1 contract read failed.
    L1ContractError,

    // ── Edge (E200–E299) ────────────────────────────────────────────
    /// GH-E200: Edge failed to register with the hive.
    EdgeRegistrationFailed,
    /// GH-E201: Transaction pipeline error.
    TxPipelineError,
    /// GH-E202: Feed correlation error.
    FeedCorrelationError,

    // ── Hive (E300–E399) ────────────────────────────────────────────
    /// GH-E300: Run orchestration failed.
    OrchestrationError,
    /// GH-E301: Edge registry operation failed.
    EdgeRegistryError,
    /// GH-E302: REST API error.
    ApiError,

    // ── Workload (E400–E499) ────────────────────────────────────────
    /// GH-E400: Transaction template construction failed.
    TemplateError,
    /// GH-E401: Rate control error.
    RateControlError,
    /// GH-E402: Account pool exhausted or nonce error.
    AccountPoolError,

    // ── Telemetry (E500–E599) ───────────────────────────────────────
    /// GH-E500: Event shipping failed.
    EventShippingError,
    /// GH-E501: Parquet storage write failed.
    StorageWriteError,
    /// GH-E502: Metric registration or recording failed.
    MetricError,

    // ── Oracle (E600–E699) ──────────────────────────────────────────
    /// GH-E600: Invariant check execution failed.
    CheckExecutionError,
    /// GH-E601: Evidence collection failed.
    EvidenceError,
    /// GH-E602: Verdict aggregation error.
    VerdictAggregationError,

    // ── Fault (E700–E799) ───────────────────────────────────────────
    /// GH-E700: Fault injection failed.
    FaultInjectionError,
    /// GH-E701: Fault reversion failed.
    FaultReversionError,
    /// GH-E702: Fault scheduling error.
    FaultSchedulingError,

    // ── Report (E800–E899) ──────────────────────────────────────────
    /// GH-E800: Report generation failed.
    ReportGenerationError,
    /// GH-E801: Regression analysis failed.
    RegressionAnalysisError,
    /// GH-E802: Template rendering failed.
    ReportTemplateError,

    // ── Scenario (E900–E949) ────────────────────────────────────────
    /// GH-E900: Scenario YAML parsing failed.
    ScenarioParseError,
    /// GH-E901: Scenario validation failed.
    ScenarioValidationError,
    /// GH-E902: Scenario compilation failed.
    ScenarioCompilationError,

    // ── Docgen (E950–E979) ──────────────────────────────────────────
    /// GH-E950: Source file parsing failed.
    SourceParseError,
    /// GH-E951: Template rendering failed.
    DocgenTemplateError,
    /// GH-E952: mdBook output write failed.
    DocgenOutputError,

    // ── Testenv (E980–E999) ─────────────────────────────────────────
    /// GH-E980: Container startup failed.
    ContainerError,
    /// GH-E981: Devnet health check failed.
    HealthCheckError,
    /// GH-E982: Contract deployment failed.
    ContractDeployError,
}

impl ErrorCode {
    /// Returns the numeric code (e.g., `1` for GH-E001).
    pub const fn numeric_code(self) -> u16 {
        match self {
            // Common
            Self::ConfigError => 1,
            Self::SerializationError => 2,
            Self::InvariantViolation => 3,
            // Nitro
            Self::RpcError => 100,
            Self::FeedError => 101,
            Self::L1ContractError => 102,
            // Edge
            Self::EdgeRegistrationFailed => 200,
            Self::TxPipelineError => 201,
            Self::FeedCorrelationError => 202,
            // Hive
            Self::OrchestrationError => 300,
            Self::EdgeRegistryError => 301,
            Self::ApiError => 302,
            // Workload
            Self::TemplateError => 400,
            Self::RateControlError => 401,
            Self::AccountPoolError => 402,
            // Telemetry
            Self::EventShippingError => 500,
            Self::StorageWriteError => 501,
            Self::MetricError => 502,
            // Oracle
            Self::CheckExecutionError => 600,
            Self::EvidenceError => 601,
            Self::VerdictAggregationError => 602,
            // Fault
            Self::FaultInjectionError => 700,
            Self::FaultReversionError => 701,
            Self::FaultSchedulingError => 702,
            // Report
            Self::ReportGenerationError => 800,
            Self::RegressionAnalysisError => 801,
            Self::ReportTemplateError => 802,
            // Scenario
            Self::ScenarioParseError => 900,
            Self::ScenarioValidationError => 901,
            Self::ScenarioCompilationError => 902,
            // Docgen
            Self::SourceParseError => 950,
            Self::DocgenTemplateError => 951,
            Self::DocgenOutputError => 952,
            // Testenv
            Self::ContainerError => 980,
            Self::HealthCheckError => 981,
            Self::ContractDeployError => 982,
        }
    }

    /// Returns the formatted code string (e.g., `"GH-E001"`).
    pub fn code(self) -> String {
        format!("GH-E{:03}", self.numeric_code())
    }

    /// Returns a short human-readable description.
    pub const fn message(self) -> &'static str {
        match self {
            // Common
            Self::ConfigError => "configuration error",
            Self::SerializationError => "serialization error",
            Self::InvariantViolation => "internal invariant violation",
            // Nitro
            Self::RpcError => "RPC error",
            Self::FeedError => "feed error",
            Self::L1ContractError => "L1 contract error",
            // Edge
            Self::EdgeRegistrationFailed => "edge registration failed",
            Self::TxPipelineError => "tx pipeline error",
            Self::FeedCorrelationError => "feed correlation error",
            // Hive
            Self::OrchestrationError => "orchestration error",
            Self::EdgeRegistryError => "edge registry error",
            Self::ApiError => "API error",
            // Workload
            Self::TemplateError => "template error",
            Self::RateControlError => "rate control error",
            Self::AccountPoolError => "account pool error",
            // Telemetry
            Self::EventShippingError => "event shipping error",
            Self::StorageWriteError => "storage write error",
            Self::MetricError => "metric error",
            // Oracle
            Self::CheckExecutionError => "invariant check failed",
            Self::EvidenceError => "evidence collection error",
            Self::VerdictAggregationError => "verdict aggregation error",
            // Fault
            Self::FaultInjectionError => "fault injection error",
            Self::FaultReversionError => "fault reversion error",
            Self::FaultSchedulingError => "fault scheduling error",
            // Report
            Self::ReportGenerationError => "report generation error",
            Self::RegressionAnalysisError => "regression analysis error",
            Self::ReportTemplateError => "template rendering error",
            // Scenario
            Self::ScenarioParseError => "scenario parse error",
            Self::ScenarioValidationError => "scenario validation error",
            Self::ScenarioCompilationError => "scenario compilation error",
            // Docgen
            Self::SourceParseError => "source parse error",
            Self::DocgenTemplateError => "docgen template error",
            Self::DocgenOutputError => "docgen output error",
            // Testenv
            Self::ContainerError => "container error",
            Self::HealthCheckError => "health check error",
            Self::ContractDeployError => "contract deployment error",
        }
    }

    /// Returns the category (originating crate) for this error code.
    pub const fn category(self) -> ErrorCategory {
        match self.numeric_code() {
            0..=99 => ErrorCategory::Common,
            100..=199 => ErrorCategory::Nitro,
            200..=299 => ErrorCategory::Edge,
            300..=399 => ErrorCategory::Hive,
            400..=499 => ErrorCategory::Workload,
            500..=599 => ErrorCategory::Telemetry,
            600..=699 => ErrorCategory::Oracle,
            700..=799 => ErrorCategory::Fault,
            800..=899 => ErrorCategory::Report,
            900..=949 => ErrorCategory::Scenario,
            950..=979 => ErrorCategory::Docgen,
            980..=999 => ErrorCategory::Testenv,
            _ => ErrorCategory::Common,
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GH-E{:03}: {}", self.numeric_code(), self.message())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_format() {
        assert_eq!(
            ErrorCode::ConfigError.to_string(),
            "GH-E001: configuration error"
        );
        assert_eq!(ErrorCode::RpcError.to_string(), "GH-E100: RPC error");
        assert_eq!(
            ErrorCode::ContainerError.to_string(),
            "GH-E980: container error"
        );
    }

    #[test]
    fn test_code_method() {
        assert_eq!(ErrorCode::ConfigError.code(), "GH-E001");
        assert_eq!(ErrorCode::FeedError.code(), "GH-E101");
        assert_eq!(ErrorCode::ScenarioParseError.code(), "GH-E900");
    }

    #[test]
    fn test_category_common() {
        assert_eq!(ErrorCode::ConfigError.category(), ErrorCategory::Common);
        assert_eq!(
            ErrorCode::SerializationError.category(),
            ErrorCategory::Common
        );
        assert_eq!(
            ErrorCode::InvariantViolation.category(),
            ErrorCategory::Common
        );
    }

    #[test]
    fn test_category_per_crate() {
        assert_eq!(ErrorCode::RpcError.category(), ErrorCategory::Nitro);
        assert_eq!(
            ErrorCode::EdgeRegistrationFailed.category(),
            ErrorCategory::Edge
        );
        assert_eq!(
            ErrorCode::OrchestrationError.category(),
            ErrorCategory::Hive
        );
        assert_eq!(ErrorCode::TemplateError.category(), ErrorCategory::Workload);
        assert_eq!(
            ErrorCode::EventShippingError.category(),
            ErrorCategory::Telemetry
        );
        assert_eq!(
            ErrorCode::CheckExecutionError.category(),
            ErrorCategory::Oracle
        );
        assert_eq!(
            ErrorCode::FaultInjectionError.category(),
            ErrorCategory::Fault
        );
        assert_eq!(
            ErrorCode::ReportGenerationError.category(),
            ErrorCategory::Report
        );
        assert_eq!(
            ErrorCode::ScenarioParseError.category(),
            ErrorCategory::Scenario
        );
        assert_eq!(
            ErrorCode::SourceParseError.category(),
            ErrorCategory::Docgen
        );
        assert_eq!(ErrorCode::ContainerError.category(), ErrorCategory::Testenv);
    }

    #[test]
    fn test_all_codes_unique() {
        let codes = [
            ErrorCode::ConfigError,
            ErrorCode::SerializationError,
            ErrorCode::InvariantViolation,
            ErrorCode::RpcError,
            ErrorCode::FeedError,
            ErrorCode::L1ContractError,
            ErrorCode::EdgeRegistrationFailed,
            ErrorCode::TxPipelineError,
            ErrorCode::FeedCorrelationError,
            ErrorCode::OrchestrationError,
            ErrorCode::EdgeRegistryError,
            ErrorCode::ApiError,
            ErrorCode::TemplateError,
            ErrorCode::RateControlError,
            ErrorCode::AccountPoolError,
            ErrorCode::EventShippingError,
            ErrorCode::StorageWriteError,
            ErrorCode::MetricError,
            ErrorCode::CheckExecutionError,
            ErrorCode::EvidenceError,
            ErrorCode::VerdictAggregationError,
            ErrorCode::FaultInjectionError,
            ErrorCode::FaultReversionError,
            ErrorCode::FaultSchedulingError,
            ErrorCode::ReportGenerationError,
            ErrorCode::RegressionAnalysisError,
            ErrorCode::ReportTemplateError,
            ErrorCode::ScenarioParseError,
            ErrorCode::ScenarioValidationError,
            ErrorCode::ScenarioCompilationError,
            ErrorCode::SourceParseError,
            ErrorCode::DocgenTemplateError,
            ErrorCode::DocgenOutputError,
            ErrorCode::ContainerError,
            ErrorCode::HealthCheckError,
            ErrorCode::ContractDeployError,
        ];
        let mut seen = std::collections::HashSet::new();
        for code in &codes {
            assert!(
                seen.insert(code.numeric_code()),
                "duplicate numeric code: {}",
                code.numeric_code()
            );
        }
    }

    #[test]
    fn test_message_non_empty() {
        let codes = [
            ErrorCode::ConfigError,
            ErrorCode::RpcError,
            ErrorCode::EdgeRegistrationFailed,
            ErrorCode::OrchestrationError,
            ErrorCode::TemplateError,
            ErrorCode::EventShippingError,
            ErrorCode::CheckExecutionError,
            ErrorCode::FaultInjectionError,
            ErrorCode::ReportGenerationError,
            ErrorCode::ScenarioParseError,
            ErrorCode::SourceParseError,
            ErrorCode::ContainerError,
        ];
        for code in &codes {
            assert!(!code.message().is_empty(), "{:?} has empty message", code);
        }
    }

    #[test]
    fn test_category_display() {
        assert_eq!(ErrorCategory::Common.to_string(), "common");
        assert_eq!(ErrorCategory::Nitro.to_string(), "nitro");
        assert_eq!(ErrorCategory::Testenv.to_string(), "testenv");
    }

    #[test]
    fn test_serialization() {
        let code = ErrorCode::ConfigError;
        let json = serde_json::to_string(&code).expect("serialize");
        assert!(json.contains("ConfigError"));

        let category = ErrorCategory::Common;
        let json = serde_json::to_string(&category).expect("serialize");
        assert!(json.contains("Common"));
    }
}
