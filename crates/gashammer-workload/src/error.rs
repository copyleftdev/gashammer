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

/// Errors from the workload engine.
///
/// Error code range: GH-E400 through GH-E499.
#[derive(Debug, Error)]
pub enum WorkloadError {
    /// GH-E400: Transaction template construction failed.
    #[error("GH-E400: template error: {0}")]
    Template(String),

    /// GH-E401: Rate control error.
    #[error("GH-E401: rate control error: {0}")]
    RateControl(String),

    /// GH-E402: Account pool exhausted or nonce error.
    #[error("GH-E402: account pool error: {0}")]
    AccountPool(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_includes_code() {
        let err = WorkloadError::Template("unknown template 'foo'".into());
        let msg = err.to_string();
        assert!(msg.contains("GH-E400"));
        assert!(msg.contains("unknown template 'foo'"));
    }
}
