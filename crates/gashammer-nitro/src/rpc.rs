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

use std::time::Duration;

use alloy::primitives::{Address, Bytes, TxHash, U256};
use alloy::providers::{Provider, RootProvider};
use alloy::rpc::types::{Block, BlockNumberOrTag, Filter, Log, TransactionReceipt};
use alloy::transports::http::Http;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::NitroError;
use crate::headers;

/// Configuration for an RPC provider connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcConfig {
    /// RPC endpoint URL.
    pub url: String,
    /// Request timeout in milliseconds.
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    /// Maximum retry attempts for transient errors.
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// Base delay for exponential backoff in milliseconds.
    #[serde(default = "default_retry_base_delay_ms")]
    pub retry_base_delay_ms: u64,
}

fn default_timeout_ms() -> u64 {
    30_000
}

fn default_max_retries() -> u32 {
    3
}

fn default_retry_base_delay_ms() -> u64 {
    500
}

impl RpcConfig {
    pub fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms)
    }

    pub fn retry_base_delay(&self) -> Duration {
        Duration::from_millis(self.retry_base_delay_ms)
    }
}

/// Nitro RPC provider wrapping an alloy HTTP provider with retry logic
/// and health checking.
pub struct NitroRpcProvider {
    provider: RootProvider,
    config: RpcConfig,
}

impl NitroRpcProvider {
    /// Connect to an RPC endpoint with GasHammer User-Agent headers.
    pub fn new(config: RpcConfig) -> Result<Self, NitroError> {
        let url: Url = config
            .url
            .parse()
            .map_err(|e| NitroError::Config(format!("invalid RPC URL: {e}")))?;

        let client = headers::http_client()?;
        let http = Http::with_client(client, url);
        let rpc_client = alloy::rpc::client::RpcClient::new(http, false);
        let provider = RootProvider::new(rpc_client);

        Ok(Self { provider, config })
    }

    /// Health check: call `eth_chainId` and verify the connection works.
    pub async fn health_check(&self) -> Result<u64, NitroError> {
        self.with_retry(|| async { self.provider.get_chain_id().await })
            .await
            .map_err(|e| NitroError::HealthCheck(e.to_string()))
    }

    /// Send a raw signed transaction to the sequencer.
    pub async fn send_raw_transaction(&self, raw_tx: &Bytes) -> Result<TxHash, NitroError> {
        let pending = self
            .with_retry(|| async { self.provider.send_raw_transaction(raw_tx).await })
            .await
            .map_err(|e| NitroError::Transport(format!("send_raw_transaction: {e}")))?;

        Ok(*pending.tx_hash())
    }

    /// Get a transaction receipt by hash.
    pub async fn get_transaction_receipt(
        &self,
        tx_hash: TxHash,
    ) -> Result<Option<TransactionReceipt>, NitroError> {
        self.with_retry(|| async { self.provider.get_transaction_receipt(tx_hash).await })
            .await
            .map_err(|e| NitroError::Transport(format!("get_transaction_receipt: {e}")))
    }

    /// Get the balance of an address at a specific block.
    pub async fn get_balance(
        &self,
        address: Address,
        block: BlockNumberOrTag,
    ) -> Result<U256, NitroError> {
        self.with_retry(|| async {
            self.provider
                .get_balance(address)
                .block_id(block.into())
                .await
        })
        .await
        .map_err(|e| NitroError::Transport(format!("get_balance: {e}")))
    }

    /// Get the nonce (transaction count) for an address.
    pub async fn get_transaction_count(
        &self,
        address: Address,
        block: BlockNumberOrTag,
    ) -> Result<u64, NitroError> {
        self.with_retry(|| async {
            self.provider
                .get_transaction_count(address)
                .block_id(block.into())
                .await
        })
        .await
        .map_err(|e| NitroError::Transport(format!("get_transaction_count: {e}")))
    }

    /// Get a block by number or tag.
    pub async fn get_block(&self, block: BlockNumberOrTag) -> Result<Option<Block>, NitroError> {
        self.with_retry(|| async { self.provider.get_block_by_number(block).await })
            .await
            .map_err(|e| NitroError::Transport(format!("get_block: {e}")))
    }

    /// Get logs matching a filter.
    pub async fn get_logs(&self, filter: &Filter) -> Result<Vec<Log>, NitroError> {
        let filter = filter.clone();
        self.with_retry(|| {
            let f = filter.clone();
            async move { self.provider.get_logs(&f).await }
        })
        .await
        .map_err(|e| NitroError::Transport(format!("get_logs: {e}")))
    }

    /// Get the current gas price.
    pub async fn gas_price(&self) -> Result<u128, NitroError> {
        self.with_retry(|| async { self.provider.get_gas_price().await })
            .await
            .map_err(|e| NitroError::Transport(format!("gas_price: {e}")))
    }

    /// Get the latest block number.
    pub async fn block_number(&self) -> Result<u64, NitroError> {
        self.with_retry(|| async { self.provider.get_block_number().await })
            .await
            .map_err(|e| NitroError::Transport(format!("block_number: {e}")))
    }

    /// Returns a reference to the underlying alloy provider.
    pub fn inner(&self) -> &RootProvider {
        &self.provider
    }

    /// Returns a reference to the config.
    pub fn config(&self) -> &RpcConfig {
        &self.config
    }

    /// Retry a fallible async operation with exponential backoff.
    async fn with_retry<F, Fut, T, E>(&self, f: F) -> Result<T, E>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        E: std::fmt::Display,
    {
        let mut last_err = None;
        for attempt in 0..=self.config.max_retries {
            match tokio::time::timeout(self.config.timeout(), f()).await {
                Ok(Ok(val)) => return Ok(val),
                Ok(Err(e)) => {
                    tracing::warn!(
                        attempt = attempt + 1,
                        max = self.config.max_retries + 1,
                        error = %e,
                        "RPC request failed, retrying"
                    );
                    last_err = Some(e);
                }
                Err(_elapsed) => {
                    tracing::warn!(
                        attempt = attempt + 1,
                        timeout_ms = self.config.timeout_ms,
                        "RPC request timed out"
                    );
                }
            }
            if attempt < self.config.max_retries {
                let delay = self.config.retry_base_delay() * 2u32.saturating_pow(attempt);
                tokio::time::sleep(delay).await;
            }
        }
        Err(last_err.expect("at least one attempt must have been made"))
    }
}

impl std::fmt::Debug for NitroRpcProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NitroRpcProvider")
            .field("url", &self.config.url)
            .field("timeout_ms", &self.config.timeout_ms)
            .field("max_retries", &self.config.max_retries)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rpc_config_defaults() {
        let config = RpcConfig {
            url: "http://localhost:8545".into(),
            timeout_ms: default_timeout_ms(),
            max_retries: default_max_retries(),
            retry_base_delay_ms: default_retry_base_delay_ms(),
        };
        assert_eq!(config.timeout(), Duration::from_secs(30));
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_base_delay(), Duration::from_millis(500));
    }

    #[test]
    fn test_provider_construction() {
        let config = RpcConfig {
            url: "http://localhost:8545".into(),
            timeout_ms: default_timeout_ms(),
            max_retries: default_max_retries(),
            retry_base_delay_ms: default_retry_base_delay_ms(),
        };
        let provider = NitroRpcProvider::new(config);
        assert!(provider.is_ok());
    }

    #[test]
    fn test_provider_invalid_url() {
        let config = RpcConfig {
            url: "not a url".into(),
            timeout_ms: default_timeout_ms(),
            max_retries: default_max_retries(),
            retry_base_delay_ms: default_retry_base_delay_ms(),
        };
        let result = NitroRpcProvider::new(config);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("GH-E107"));
    }

    #[test]
    fn test_provider_debug() {
        let config = RpcConfig {
            url: "http://localhost:8545".into(),
            timeout_ms: default_timeout_ms(),
            max_retries: default_max_retries(),
            retry_base_delay_ms: default_retry_base_delay_ms(),
        };
        let provider = NitroRpcProvider::new(config).unwrap();
        let debug = format!("{provider:?}");
        assert!(debug.contains("localhost:8545"));
    }

    #[test]
    fn test_rpc_config_serialization() {
        let config = RpcConfig {
            url: "http://localhost:8545".into(),
            timeout_ms: 10_000,
            max_retries: 5,
            retry_base_delay_ms: 200,
        };
        let json = serde_json::to_string(&config).expect("serialize");
        let parsed: RpcConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.url, config.url);
        assert_eq!(parsed.max_retries, 5);
        assert_eq!(parsed.timeout_ms, 10_000);
    }
}
