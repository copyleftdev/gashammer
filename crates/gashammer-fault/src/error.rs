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

/// Errors from the fault injection framework.
///
/// Error code range: GH-E700 through GH-E799.
#[derive(Debug, Error)]
pub enum FaultError {
    /// GH-E700: Fault injection failed.
    #[error("GH-E700: injection error: {0}")]
    Injection(String),

    /// GH-E701: Fault reversion failed.
    #[error("GH-E701: reversion error: {0}")]
    Reversion(String),

    /// GH-E702: Fault scheduling error.
    #[error("GH-E702: scheduling error: {0}")]
    Scheduling(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_includes_code() {
        let err = FaultError::Injection("permission denied for netem".into());
        let msg = err.to_string();
        assert!(msg.contains("GH-E700"));
        assert!(msg.contains("permission denied for netem"));
    }
}
