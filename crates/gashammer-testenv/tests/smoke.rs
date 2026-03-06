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

//! Smoke integration tests — verify the Nitro devnet boots and responds.
//!
//! These tests require Docker. Run with:
//!   cargo test --test smoke -- --test-threads=1
//!
//! Ref: RFC-0013 §7.1.

use alloy::network::TransactionBuilder;
use alloy::primitives::{Address, U256};
use alloy::providers::{Provider, ProviderBuilder};

use gashammer_testenv::devnet::NitroDevnet;
use gashammer_testenv::funding;

/// Helper: check if Docker is available.
fn docker_available() -> bool {
    std::process::Command::new("docker")
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[tokio::test]
async fn devnet_boots_and_rpc_responds() {
    if !docker_available() {
        eprintln!("SKIP: Docker not available");
        return;
    }

    let devnet = NitroDevnet::minimal().await.expect("devnet should boot");

    // Health check should pass
    devnet
        .health_check()
        .await
        .expect("health check should pass");

    // Sequencer RPC responds with a chain ID
    let provider = ProviderBuilder::new().on_http(devnet.sequencer_rpc_url().parse().unwrap());

    let chain_id = provider.get_chain_id().await.expect("should get chain ID");
    assert_eq!(chain_id, 412346, "should be Nitro devnet chain ID");

    // Should be able to get block number
    let block_num = provider
        .get_block_number()
        .await
        .expect("should get block number");

    // Block number might be 0 or higher depending on devnet init
    tracing::info!(chain_id, block_num, "devnet is responding");
}

#[tokio::test]
async fn devnet_funds_test_accounts() {
    if !docker_available() {
        eprintln!("SKIP: Docker not available");
        return;
    }

    let devnet = NitroDevnet::minimal().await.expect("devnet should boot");

    // Fund 3 test accounts
    let accounts = funding::fund_test_accounts(devnet.sequencer_rpc_url(), 3)
        .await
        .expect("should fund accounts");

    assert_eq!(accounts.len(), 3);

    // Verify each account has balance
    let provider = ProviderBuilder::new().on_http(devnet.sequencer_rpc_url().parse().unwrap());

    for account in &accounts {
        let balance = provider
            .get_balance(account.address)
            .await
            .expect("should get balance");
        assert!(balance > U256::ZERO, "account should have balance");
        assert_ne!(account.address, Address::ZERO, "address should not be zero");
    }

    tracing::info!(count = accounts.len(), "all accounts funded and verified");
}

#[tokio::test]
async fn devnet_submit_simple_transfer() {
    if !docker_available() {
        eprintln!("SKIP: Docker not available");
        return;
    }

    let devnet = NitroDevnet::minimal().await.expect("devnet should boot");

    // Fund 2 accounts
    let accounts = funding::fund_test_accounts(devnet.sequencer_rpc_url(), 2)
        .await
        .expect("should fund accounts");

    let sender = &accounts[0];
    let receiver = &accounts[1];

    // Submit a simple ETH transfer
    let provider = ProviderBuilder::new()
        .wallet(alloy::network::EthereumWallet::from(sender.signer.clone()))
        .on_http(devnet.sequencer_rpc_url().parse().unwrap());

    let transfer_amount = U256::from(1_000_000_000_000_000_000u64); // 1 ETH

    let tx = alloy::rpc::types::TransactionRequest::default()
        .with_to(receiver.address)
        .with_value(transfer_amount);

    let receipt = provider
        .send_transaction(tx)
        .await
        .expect("should send tx")
        .get_receipt()
        .await
        .expect("should get receipt");

    assert!(receipt.status(), "tx should succeed");
    // Nitro L2 gas includes ArbOS overhead, so a simple transfer uses more than 21000.
    assert!(
        receipt.gas_used >= 21000,
        "should consume at least base gas"
    );

    // Verify receiver balance increased
    let read_provider = ProviderBuilder::new().on_http(devnet.sequencer_rpc_url().parse().unwrap());

    let receiver_balance = read_provider
        .get_balance(receiver.address)
        .await
        .expect("should get balance");

    assert!(
        receiver_balance > accounts[1].balance,
        "receiver balance should have increased"
    );

    tracing::info!(gas_used = receipt.gas_used, "simple transfer confirmed");
}
