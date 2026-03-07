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

/// Errors from the scenario definition language parser and compiler.
///
/// Error code range: GH-E900 through GH-E949.
#[derive(Debug, Error)]
pub enum ScenarioError {
    /// GH-E900: Scenario YAML parsing failed.
    #[error("GH-E900: parse error: {0}")]
    Parse(String),

    /// GH-E901: Scenario validation failed.
    #[error("GH-E901: validation error: {0}")]
    Validation(String),

    /// GH-E902: Scenario compilation failed.
    #[error("GH-E902: compilation error: {0}")]
    Compilation(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_includes_code() {
        let err = ScenarioError::Validation("phase duration must be positive".into());
        let msg = err.to_string();
        assert!(msg.contains("GH-E901"));
        assert!(msg.contains("phase duration must be positive"));
    }
}
