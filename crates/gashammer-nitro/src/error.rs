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

/// Errors from Nitro protocol adapters.
///
/// Error code range: GH-E100 through GH-E199.
#[derive(Debug, Error)]
pub enum NitroError {
    /// GH-E100: JSON-RPC transport error.
    #[error("GH-E100: RPC transport error: {0}")]
    Transport(String),

    /// GH-E101: Sequencer feed connection or parsing failed.
    #[error("GH-E101: feed error: {0}")]
    Feed(String),

    /// GH-E102: L1 contract read failed.
    #[error("GH-E102: L1 contract error: {0}")]
    L1Contract(String),

    /// GH-E103: RPC request timed out.
    #[error("GH-E103: RPC timeout: {0}")]
    Timeout(String),

    /// GH-E104: Rate limited by the target node.
    #[error("GH-E104: rate limited: {0}")]
    RateLimited(String),

    /// GH-E105: Invalid RPC response.
    #[error("GH-E105: invalid response: {0}")]
    InvalidResponse(String),

    /// GH-E106: Health check failed.
    #[error("GH-E106: health check failed: {0}")]
    HealthCheck(String),

    /// GH-E107: Invalid configuration.
    #[error("GH-E107: invalid config: {0}")]
    Config(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_includes_code() {
        let err = NitroError::Transport("connection refused".into());
        let msg = err.to_string();
        assert!(msg.contains("GH-E100"));
        assert!(msg.contains("connection refused"));
    }

    #[test]
    fn test_timeout_error() {
        let err = NitroError::Timeout("eth_getTransactionReceipt after 30s".into());
        assert!(err.to_string().contains("GH-E103"));
    }

    #[test]
    fn test_rate_limited_error() {
        let err = NitroError::RateLimited("429 Too Many Requests".into());
        assert!(err.to_string().contains("GH-E104"));
    }
}
