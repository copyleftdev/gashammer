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

use thiserror::Error;

/// Errors from the correctness oracle.
///
/// Error code range: GH-E600 through GH-E699.
#[derive(Debug, Error)]
pub enum OracleError {
    /// GH-E600: Invariant check execution failed.
    #[error("GH-E600: invariant check failed: {0}")]
    CheckExecution(String),

    /// GH-E601: Evidence collection failed.
    #[error("GH-E601: evidence collection error: {0}")]
    Evidence(String),

    /// GH-E602: Verdict aggregation error.
    #[error("GH-E602: verdict aggregation error: {0}")]
    VerdictAggregation(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_includes_code() {
        let err = OracleError::CheckExecution("state query timeout".into());
        let msg = err.to_string();
        assert!(msg.contains("GH-E600"));
        assert!(msg.contains("state query timeout"));
    }
}
