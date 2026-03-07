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

/// Errors from the telemetry pipeline.
///
/// Error code range: GH-E500 through GH-E599.
#[derive(Debug, Error)]
pub enum TelemetryError {
    /// GH-E500: Event shipping failed.
    #[error("GH-E500: event shipping error: {0}")]
    Shipping(String),

    /// GH-E501: Parquet storage write failed.
    #[error("GH-E501: storage write error: {0}")]
    StorageWrite(String),

    /// GH-E502: Metric registration or recording failed.
    #[error("GH-E502: metric error: {0}")]
    Metric(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_includes_code() {
        let err = TelemetryError::Shipping("channel full".into());
        let msg = err.to_string();
        assert!(msg.contains("GH-E500"));
        assert!(msg.contains("channel full"));
    }
}
