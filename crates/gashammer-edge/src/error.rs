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

/// Errors from the edge runtime.
///
/// Error code range: GH-E200 through GH-E299.
#[derive(Debug, Error)]
pub enum EdgeError {
    /// GH-E200: Edge failed to register with the hive.
    #[error("GH-E200: registration failed: {0}")]
    Registration(String),

    /// GH-E201: Transaction pipeline error.
    #[error("GH-E201: tx pipeline error: {0}")]
    TxPipeline(String),

    /// GH-E202: Feed correlation error.
    #[error("GH-E202: feed correlation error: {0}")]
    FeedCorrelation(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_includes_code() {
        let err = EdgeError::Registration("hive unreachable".into());
        let msg = err.to_string();
        assert!(msg.contains("GH-E200"));
        assert!(msg.contains("hive unreachable"));
    }
}
