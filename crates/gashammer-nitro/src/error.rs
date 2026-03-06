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

/// Errors from Nitro protocol adapters.
///
/// Error code range: GH-E100 through GH-E199.
#[derive(Debug, Error)]
pub enum NitroError {
    /// GH-E100: JSON-RPC call failed.
    #[error("GH-E100: RPC error: {0}")]
    Rpc(String),

    /// GH-E101: Sequencer feed connection or parsing failed.
    #[error("GH-E101: feed error: {0}")]
    Feed(String),

    /// GH-E102: L1 contract read failed.
    #[error("GH-E102: L1 contract error: {0}")]
    L1Contract(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_includes_code() {
        let err = NitroError::Rpc("connection refused".into());
        let msg = err.to_string();
        assert!(msg.contains("GH-E100"));
        assert!(msg.contains("connection refused"));
    }
}
