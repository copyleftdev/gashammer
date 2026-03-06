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

//! Account pre-funding and contract deployment for the Nitro devnet.
//!
//! Uses the Nitro devnet dev account to fund test accounts and deploy
//! test contracts. Ref: RFC-0013 §5.

use alloy::network::TransactionBuilder;
use alloy::primitives::{Address, Bytes, U256};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::TransactionRequest;
use alloy::signers::local::PrivateKeySigner;

use crate::contracts::{self, TestContracts};
use crate::error::TestenvError;

/// A funded test account with its private key.
#[derive(Debug, Clone)]
pub struct FundedAccount {
    pub address: Address,
    pub signer: PrivateKeySigner,
    pub balance: U256,
}

/// Fund `count` test accounts with 100 ETH each using the Nitro dev account.
///
/// The Nitro devnet (via `--chain.dev-init`) pre-funds a dev account with
/// a well-known private key. We use it to send ETH to deterministic test accounts.
pub async fn fund_test_accounts(
    rpc_url: &str,
    count: usize,
) -> Result<Vec<FundedAccount>, TestenvError> {
    let dev_signer: PrivateKeySigner = NITRO_DEV_PRIVATE_KEY
        .parse()
        .map_err(|e| TestenvError::Funding(format!("invalid dev key: {e}")))?;

    let provider = ProviderBuilder::new()
        .wallet(alloy::network::EthereumWallet::from(dev_signer.clone()))
        .on_http(
            rpc_url
                .parse()
                .map_err(|e| TestenvError::Funding(format!("invalid RPC URL: {e}")))?,
        );

    let funding_amount = U256::from(100) * U256::from(10).pow(U256::from(18)); // 100 ETH

    let mut accounts = Vec::with_capacity(count);

    for i in 0..count {
        // Deterministic key derivation from index
        let key_bytes = alloy::primitives::keccak256(format!("gashammer-test-account-{i}"));
        let signer: PrivateKeySigner =
            alloy::signers::local::PrivateKeySigner::from_bytes(&key_bytes)
                .map_err(|e| TestenvError::Funding(format!("key derivation failed: {e}")))?;

        let address = signer.address();

        let tx = TransactionRequest::default()
            .with_to(address)
            .with_value(funding_amount);

        provider
            .send_transaction(tx)
            .await
            .map_err(|e| {
                TestenvError::Funding(format!("fund tx send failed for account {i}: {e}"))
            })?
            .get_receipt()
            .await
            .map_err(|e| {
                TestenvError::Funding(format!("fund tx receipt failed for account {i}: {e}"))
            })?;

        accounts.push(FundedAccount {
            address,
            signer,
            balance: funding_amount,
        });
    }

    tracing::info!(count = accounts.len(), "pre-funded test accounts");
    Ok(accounts)
}

/// Deploy a contract and return its deployed address.
pub async fn deploy_contract(
    rpc_url: &str,
    deployer: &PrivateKeySigner,
    bytecode: &[u8],
) -> Result<Address, TestenvError> {
    let provider = ProviderBuilder::new()
        .wallet(alloy::network::EthereumWallet::from(deployer.clone()))
        .on_http(
            rpc_url
                .parse()
                .map_err(|e| TestenvError::Deployment(format!("invalid RPC URL: {e}")))?,
        );

    let tx = TransactionRequest::default().with_input(Bytes::copy_from_slice(bytecode));

    let receipt = provider
        .send_transaction(tx)
        .await
        .map_err(|e| TestenvError::Deployment(format!("deploy tx send failed: {e}")))?
        .get_receipt()
        .await
        .map_err(|e| TestenvError::Deployment(format!("deploy tx receipt failed: {e}")))?;

    let address = receipt
        .contract_address
        .ok_or_else(|| TestenvError::Deployment("no contract address in receipt".into()))?;

    Ok(address)
}

/// Deploy all test contracts and return their addresses.
pub async fn deploy_test_contracts(
    rpc_url: &str,
    deployer: &PrivateKeySigner,
) -> Result<TestContracts, TestenvError> {
    tracing::info!("deploying test contracts");

    let counter = deploy_contract(rpc_url, deployer, contracts::COUNTER_BYTECODE).await?;
    tracing::debug!(%counter, "Counter deployed");

    let erc20 = deploy_contract(rpc_url, deployer, contracts::ERC20_BYTECODE).await?;
    tracing::debug!(%erc20, "GasHammerERC20 deployed");

    let gas_burner = deploy_contract(rpc_url, deployer, contracts::GAS_BURNER_BYTECODE).await?;
    tracing::debug!(%gas_burner, "GasBurner deployed");

    let storage_writer =
        deploy_contract(rpc_url, deployer, contracts::STORAGE_WRITER_BYTECODE).await?;
    tracing::debug!(%storage_writer, "StorageWriter deployed");

    let reverter = deploy_contract(rpc_url, deployer, contracts::REVERTER_BYTECODE).await?;
    tracing::debug!(%reverter, "Reverter deployed");

    let event_emitter =
        deploy_contract(rpc_url, deployer, contracts::EVENT_EMITTER_BYTECODE).await?;
    tracing::debug!(%event_emitter, "EventEmitter deployed");

    let contention_target =
        deploy_contract(rpc_url, deployer, contracts::CONTENTION_TARGET_BYTECODE).await?;
    tracing::debug!(%contention_target, "ContentionTarget deployed");

    tracing::info!("all test contracts deployed");

    Ok(TestContracts {
        counter: counter.into_array(),
        erc20: erc20.into_array(),
        gas_burner: gas_burner.into_array(),
        storage_writer: storage_writer.into_array(),
        reverter: reverter.into_array(),
        event_emitter: event_emitter.into_array(),
        contention_target: contention_target.into_array(),
    })
}

/// Nitro devnet well-known dev account private key.
/// This is public knowledge — used only for devnet testing.
const NITRO_DEV_PRIVATE_KEY: &str =
    "b6b15c8cb491557369f3c7d2c287b053eb229daa9c22138887752191c9520659";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dev_key_parses() {
        let signer: PrivateKeySigner = NITRO_DEV_PRIVATE_KEY.parse().expect("valid key");
        assert_ne!(signer.address(), Address::ZERO);
    }

    #[test]
    fn test_deterministic_key_derivation() {
        let key1 = alloy::primitives::keccak256("gashammer-test-account-0");
        let key2 = alloy::primitives::keccak256("gashammer-test-account-0");
        assert_eq!(key1, key2);

        let key3 = alloy::primitives::keccak256("gashammer-test-account-1");
        assert_ne!(key1, key3);
    }
}
