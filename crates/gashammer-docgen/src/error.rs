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

/// Errors from the documentation engine.
///
/// Error code range: GH-E950 through GH-E979.
#[derive(Debug, Error)]
pub enum DocgenError {
    /// GH-E950: Source file parsing failed.
    #[error("GH-E950: source parse error: {0}")]
    SourceParse(String),

    /// GH-E951: Template rendering failed.
    #[error("GH-E951: template error: {0}")]
    Template(String),

    /// GH-E952: mdBook output write failed.
    #[error("GH-E952: output error: {0}")]
    Output(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_includes_code() {
        let err = DocgenError::SourceParse("unexpected token at line 42".into());
        let msg = err.to_string();
        assert!(msg.contains("GH-E950"));
        assert!(msg.contains("unexpected token at line 42"));
    }
}
