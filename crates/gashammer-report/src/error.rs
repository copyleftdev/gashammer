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

use thiserror::Error;

/// Errors from the reporting engine.
///
/// Error code range: GH-E800 through GH-E899.
#[derive(Debug, Error)]
pub enum ReportError {
    /// GH-E800: Report generation failed.
    #[error("GH-E800: report generation error: {0}")]
    Generation(String),

    /// GH-E801: Regression analysis failed.
    #[error("GH-E801: regression analysis error: {0}")]
    Regression(String),

    /// GH-E802: Template rendering failed.
    #[error("GH-E802: template rendering error: {0}")]
    Template(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_includes_code() {
        let err = ReportError::Generation("missing telemetry data".into());
        let msg = err.to_string();
        assert!(msg.contains("GH-E800"));
        assert!(msg.contains("missing telemetry data"));
    }
}
