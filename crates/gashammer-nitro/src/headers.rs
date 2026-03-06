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

//! HTTP header DNA provenance for all outbound requests.
//!
//! Ensures every HTTP and WebSocket request from GasHammer carries
//! identifying headers per RFC-0011 §4.2–4.3.

use gashammer_common::build_info::{GASHAMMER_BUILD_SHA, GASHAMMER_URL, GASHAMMER_VERSION};

/// Custom header name for GasHammer version in gRPC metadata / HTTP responses.
pub const X_GASHAMMER_VERSION: &str = "x-gashammer-version";

/// Custom header name for GasHammer build SHA in gRPC metadata / HTTP responses.
pub const X_GASHAMMER_BUILD: &str = "x-gashammer-build";

/// Header value for `X-Powered-By` on hive REST responses.
pub fn x_powered_by() -> String {
    format!("GasHammer/{GASHAMMER_VERSION}")
}

/// Returns the User-Agent string for all outbound HTTP/WS requests.
///
/// Format: `GasHammer/<version> (<url>; <sha>)`
pub fn user_agent() -> String {
    format!("GasHammer/{GASHAMMER_VERSION} ({GASHAMMER_URL}; {GASHAMMER_BUILD_SHA})")
}

/// Returns a `tokio_tungstenite::tungstenite::http::Request` with the
/// User-Agent header set, suitable for WebSocket `connect_async`.
pub fn ws_request(
    url: &str,
) -> Result<tokio_tungstenite::tungstenite::http::Request<()>, crate::error::NitroError> {
    tokio_tungstenite::tungstenite::http::Request::builder()
        .uri(url)
        .header("User-Agent", user_agent())
        .header(X_GASHAMMER_VERSION, GASHAMMER_VERSION)
        .header(X_GASHAMMER_BUILD, GASHAMMER_BUILD_SHA)
        .body(())
        .map_err(|e| crate::error::NitroError::Config(format!("failed to build WS request: {e}")))
}

/// Build a `reqwest::Client` with the GasHammer User-Agent and custom headers.
pub fn http_client() -> Result<reqwest::Client, crate::error::NitroError> {
    use reqwest::header::{HeaderMap, HeaderValue};

    let mut headers = HeaderMap::new();
    if let Ok(v) = HeaderValue::from_str(GASHAMMER_VERSION) {
        headers.insert(X_GASHAMMER_VERSION, v);
    }
    if let Ok(v) = HeaderValue::from_str(GASHAMMER_BUILD_SHA) {
        headers.insert(X_GASHAMMER_BUILD, v);
    }

    reqwest::Client::builder()
        .user_agent(user_agent())
        .default_headers(headers)
        .build()
        .map_err(|e| crate::error::NitroError::Config(format!("failed to build HTTP client: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_agent_format() {
        let ua = user_agent();
        assert!(ua.starts_with("GasHammer/"));
        assert!(ua.contains("github.com/copyleftdev/gashammer"));
    }

    #[test]
    fn test_x_powered_by() {
        let xpb = x_powered_by();
        assert!(xpb.starts_with("GasHammer/"));
        assert!(xpb.contains(GASHAMMER_VERSION));
    }

    #[test]
    fn test_ws_request_has_user_agent() {
        let req = ws_request("ws://localhost:9642").expect("build request");
        let ua = req.headers().get("User-Agent").expect("User-Agent header");
        assert!(ua.to_str().unwrap().starts_with("GasHammer/"));
    }

    #[test]
    fn test_ws_request_has_custom_headers() {
        let req = ws_request("ws://localhost:9642").expect("build request");
        let ver = req
            .headers()
            .get(X_GASHAMMER_VERSION)
            .expect("version header");
        assert_eq!(ver.to_str().unwrap(), GASHAMMER_VERSION);

        let build = req.headers().get(X_GASHAMMER_BUILD).expect("build header");
        assert_eq!(build.to_str().unwrap(), GASHAMMER_BUILD_SHA);
    }

    #[test]
    fn test_ws_request_invalid_url() {
        // An empty URL should still build a request (URI validation is lax).
        // But a truly invalid URI should fail.
        let result = ws_request("not a valid url with spaces");
        assert!(result.is_err());
    }

    #[test]
    fn test_http_client_construction() {
        let client = http_client();
        assert!(client.is_ok());
    }

    #[test]
    fn test_header_constants() {
        assert_eq!(X_GASHAMMER_VERSION, "x-gashammer-version");
        assert_eq!(X_GASHAMMER_BUILD, "x-gashammer-build");
    }
}
