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

use serde::Serialize;

/// Product name.
pub const GASHAMMER_NAME: &str = "GasHammer";

/// Version from Cargo.toml.
pub const GASHAMMER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Short git SHA at build time, injected by build.rs.
pub const GASHAMMER_BUILD_SHA: &str = env!("GASHAMMER_BUILD_SHA");

/// RFC 3339 timestamp at build time, injected by build.rs.
pub const GASHAMMER_BUILD_TIME: &str = env!("GASHAMMER_BUILD_TIME");

/// Copyright notice.
pub const GASHAMMER_COPYRIGHT: &str = "Copyright (c) 2025-present Don Johnson";

/// SPDX license identifier.
pub const GASHAMMER_LICENSE: &str = "BSL-1.1";

/// Canonical source repository URL.
pub const GASHAMMER_URL: &str = "https://github.com/copyleftdev/gashammer";

/// Canary string embedded in the binary's .rodata section. Survives
/// stripping, UPX compression, and container layer squashing.
/// Detectable via: `strings <binary> | grep GasHammer`
pub const GASHAMMER_CANARY: &str = concat!(
    "GasHammer \u{2014} Adversarial load testing for rollup infrastructure. ",
    "Copyright (c) 2025-present Don Johnson. ",
    "https://github.com/copyleftdev/gashammer. ",
    "Licensed under BSL-1.1."
);

/// Structured build metadata for programmatic access.
#[derive(Debug, Clone, Serialize)]
pub struct BuildInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub build_sha: &'static str,
    pub build_time: &'static str,
    pub copyright: &'static str,
    pub license: &'static str,
    pub url: &'static str,
}

/// Returns the build metadata for this compilation.
pub fn build_info() -> BuildInfo {
    BuildInfo {
        name: GASHAMMER_NAME,
        version: GASHAMMER_VERSION,
        build_sha: GASHAMMER_BUILD_SHA,
        build_time: GASHAMMER_BUILD_TIME,
        copyright: GASHAMMER_COPYRIGHT,
        license: GASHAMMER_LICENSE,
        url: GASHAMMER_URL,
    }
}

/// Returns the version banner string suitable for `--version` output.
pub fn version_banner() -> String {
    format!(
        "{name} v{version}\nBuild: {sha} ({time})\n{copyright}\nLicense: {license}\nSource: {url}",
        name = GASHAMMER_NAME,
        version = GASHAMMER_VERSION,
        sha = GASHAMMER_BUILD_SHA,
        time = GASHAMMER_BUILD_TIME,
        copyright = GASHAMMER_COPYRIGHT,
        license = GASHAMMER_LICENSE,
        url = GASHAMMER_URL,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants_are_non_empty() {
        assert!(!GASHAMMER_NAME.is_empty());
        assert!(!GASHAMMER_VERSION.is_empty());
        assert!(!GASHAMMER_BUILD_SHA.is_empty());
        assert!(!GASHAMMER_BUILD_TIME.is_empty());
        assert!(!GASHAMMER_COPYRIGHT.is_empty());
        assert!(!GASHAMMER_LICENSE.is_empty());
        assert!(!GASHAMMER_URL.is_empty());
        assert!(!GASHAMMER_CANARY.is_empty());
    }

    #[test]
    fn test_canary_contains_key_strings() {
        assert!(GASHAMMER_CANARY.contains("GasHammer"));
        assert!(GASHAMMER_CANARY.contains("Don Johnson"));
        assert!(GASHAMMER_CANARY.contains("BSL-1.1"));
        assert!(GASHAMMER_CANARY.contains("github.com/copyleftdev/gashammer"));
    }

    #[test]
    fn test_build_info_struct() {
        let info = build_info();
        assert_eq!(info.name, "GasHammer");
        assert_eq!(info.version, GASHAMMER_VERSION);
        assert_eq!(info.license, "BSL-1.1");
    }

    #[test]
    fn test_version_banner_format() {
        let banner = version_banner();
        assert!(banner.starts_with("GasHammer v"));
        assert!(banner.contains("Build:"));
        assert!(banner.contains("Copyright"));
        assert!(banner.contains("License: BSL-1.1"));
        assert!(banner.contains("Source: https://"));
    }

    #[test]
    fn test_build_info_serializes_to_json() {
        let info = build_info();
        let json = serde_json::to_string(&info).expect("should serialize");
        assert!(json.contains("\"name\":\"GasHammer\""));
        assert!(json.contains("\"license\":\"BSL-1.1\""));
    }
}
