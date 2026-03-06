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

/// Errors from the testcontainers devnet orchestrator.
///
/// Error code range: GH-E980 through GH-E999.
#[derive(Debug, Error)]
pub enum TestenvError {
    /// GH-E980: Container startup failed.
    #[error("GH-E980: container error: {0}")]
    Container(String),

    /// GH-E981: Devnet health check failed.
    #[error("GH-E981: health check error: {0}")]
    HealthCheck(String),

    /// GH-E982: Contract deployment failed.
    #[error("GH-E982: contract deployment error: {0}")]
    ContractDeploy(String),

    /// GH-E983: Account funding failed.
    #[error("GH-E983: funding error: {0}")]
    Funding(String),

    /// GH-E984: Contract deployment failed.
    #[error("GH-E984: deployment error: {0}")]
    Deployment(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_includes_code() {
        let err = TestenvError::Container("docker not available".into());
        let msg = err.to_string();
        assert!(msg.contains("GH-E980"));
        assert!(msg.contains("docker not available"));
    }
}
