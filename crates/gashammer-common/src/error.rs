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

/// Top-level error type for the GasHammer platform.
///
/// Error codes follow the GH-Exxx scheme. Range E001-E099 is reserved
/// for gashammer-common.
#[derive(Debug, Error)]
pub enum CommonError {
    /// GH-E001: Configuration loading or validation failed.
    #[error("GH-E001: configuration error: {0}")]
    Config(String),

    /// GH-E002: Serialization or deserialization failed.
    #[error("GH-E002: serialization error: {0}")]
    Serialization(String),

    /// GH-E003: An internal invariant was violated.
    #[error("GH-E003: internal invariant violation: {0}")]
    Invariant(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_includes_code() {
        let err = CommonError::Config("missing field 'listen_addr'".into());
        let msg = err.to_string();
        assert!(msg.contains("GH-E001"));
        assert!(msg.contains("missing field 'listen_addr'"));
    }
}
