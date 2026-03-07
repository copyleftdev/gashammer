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

/// Errors from the hive control plane.
///
/// Error code range: GH-E300 through GH-E399.
#[derive(Debug, Error)]
pub enum HiveError {
    /// GH-E300: Run orchestration failed.
    #[error("GH-E300: orchestration error: {0}")]
    Orchestration(String),

    /// GH-E301: Edge registry operation failed.
    #[error("GH-E301: edge registry error: {0}")]
    EdgeRegistry(String),

    /// GH-E302: REST API error.
    #[error("GH-E302: API error: {0}")]
    Api(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_includes_code() {
        let err = HiveError::Orchestration("no edges available".into());
        let msg = err.to_string();
        assert!(msg.contains("GH-E300"));
        assert!(msg.contains("no edges available"));
    }
}
