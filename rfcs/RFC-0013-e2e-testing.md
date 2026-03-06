# RFC-0013: End-to-End Testing with Testcontainers & Nitro Devnet

## Status

Draft

## Author

Don Johnson

## Depends On

- RFC-0001 (Nitro Integration Surface)
- RFC-0002 (Technology Stack)
- RFC-0004 (Edge Runtime)
- RFC-0005 (Workload Engine)

## Summary

GasHammer is a testing tool. A testing tool that can't be tested against the real thing is a liability. This RFC defines an end-to-end testing strategy using the **Testcontainers** framework to programmatically spin up a complete Arbitrum Nitro devnet — L1 chain, sequencer, batch poster, feed relay, and validator — in Docker containers for every test run. Every `cargo test` invocation that touches integration tests gets a fresh, isolated, real Nitro stack. No mocks. No fakes. The real thing.

---

## 1. Design Principles

1. **Real protocols, real code paths** — integration tests talk to actual Nitro nodes over actual JSON-RPC and WebSocket, not mocked interfaces
2. **Hermetic and reproducible** — each test run gets a fresh devnet; no shared state between test runs
3. **Fast enough for CI** — devnet startup must complete in <60 seconds; the full E2E suite must complete in <10 minutes
4. **No external dependencies** — tests need only Docker; no cloud accounts, no shared staging environments
5. **Fail loudly** — if a container crashes or a health check fails, the test fails with clear diagnostics, not a mysterious timeout

---

## 2. Nitro Devnet Topology

### 2.1 Reference: nitro-testnode

Offchain Labs maintains [`nitro-testnode`](https://github.com/OffchainLabs/nitro-testnode) — a docker-compose setup for local Nitro development. Our testcontainers setup mirrors this topology:

```text
┌─────────────────────────────────────────────────────────┐
│                   Docker Network: nitro-devnet          │
│                                                         │
│  ┌──────────┐    ┌─────────────┐    ┌───────────────┐  │
│  │  L1 Geth │◄───│  Sequencer  │───►│  Feed Relay   │  │
│  │  :8545   │    │  :8547      │    │  :9642        │  │
│  └──────────┘    └──────┬──────┘    └───────────────┘  │
│                         │                               │
│                  ┌──────┴──────┐                        │
│                  │ Batch Poster│                        │
│                  │ (embedded)  │                        │
│                  └─────────────┘                        │
│                                                         │
│  ┌──────────────┐  ┌──────────────┐                    │
│  │  Validator   │  │  Staker      │                    │
│  │  (optional)  │  │  (optional)  │                    │
│  └──────────────┘  └──────────────┘                    │
└─────────────────────────────────────────────────────────┘
```

### 2.2 Container Images

| Component | Image | Source |
| --- | --- | --- |
| **L1 Geth** | `ethereum/client-go:stable` | Official Geth; runs in dev mode with instant finality |
| **Nitro Sequencer** | `offchainlabs/nitro-node:latest` | Offchain Labs official; includes sequencer + batch poster |
| **Feed Relay** | `offchainlabs/nitro-node:latest` | Same image, different entrypoint (`relay` mode) |
| **Validator** | `offchainlabs/nitro-node:latest` | Same image, validation mode |
| **Token Bridge** | `offchainlabs/token-bridge-creator:latest` | Optional; needed for L1↔L2 bridge testing |

### 2.3 Minimal vs Full Topology

| Profile | Components | Startup Time | Use Case |
| --- | --- | --- | --- |
| **Minimal** | L1 Geth + Sequencer + Feed Relay | ~20s | Unit-integration tests, tx submission, feed consumption |
| **Standard** | Minimal + Batch Poster (separate) + Validator | ~40s | Full lifecycle tests, batch posting, correctness checks |
| **Full** | Standard + Token Bridge + Staker | ~60s | Complete E2E, cross-layer, BoLD assertion tests |

Most tests use **Minimal**. CI nightly runs use **Standard**. Release gate runs use **Full**.

---

## 3. Testcontainers Implementation

### 3.1 Crate: `gashammer-testenv`

```text
crates/gashammer-testenv/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── devnet.rs          # NitroDevnet orchestrator
│   ├── containers/
│   │   ├── mod.rs
│   │   ├── l1_geth.rs     # L1 Geth container definition
│   │   ├── sequencer.rs   # Nitro sequencer container
│   │   ├── feed_relay.rs  # Feed relay container
│   │   ├── validator.rs   # Validator container
│   │   └── bridge.rs      # Token bridge (optional)
│   ├── genesis.rs         # Chain genesis + L1 contract deployment
│   ├── funding.rs         # Account pre-funding
│   ├── healthcheck.rs     # Container readiness probes
│   └── fixtures.rs        # Test fixtures and helpers
└── tests/
    └── smoke.rs           # Self-test: does the devnet boot?
```

### 3.2 Dependencies

```toml
[package]
name = "gashammer-testenv"
version = "0.1.0"
edition = "2021"

[dependencies]
testcontainers = "0.23"
testcontainers-modules = "0.11"
tokio = { version = "1", features = ["full"] }
alloy = { version = "0.9", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
tempfile = "3"
rand = "0.8"
hex = "0.4"

[dev-dependencies]
tokio-test = "0.4"
```

### 3.3 Container Definitions

#### L1 Geth

```rust
use testcontainers::{core::WaitFor, Image};

const L1_GETH_IMAGE: &str = "ethereum/client-go";
const L1_GETH_TAG: &str = "stable";

pub struct L1Geth {
    chain_id: u64,
    block_time: u64,
}

impl L1Geth {
    pub fn new() -> Self {
        Self {
            chain_id: 1337,
            block_time: 1, // 1-second blocks for fast L1 finality
        }
    }
}

impl Image for L1Geth {
    fn name(&self) -> &str {
        L1_GETH_IMAGE
    }

    fn tag(&self) -> &str {
        L1_GETH_TAG
    }

    fn ready_conditions(&self) -> Vec<WaitFor> {
        vec![WaitFor::message_on_stderr("HTTP server started")]
    }

    fn cmd(&self) -> impl IntoIterator<Item = impl Into<String>> {
        vec![
            "--dev".to_string(),
            format!("--dev.period={}", self.block_time),
            "--http".to_string(),
            "--http.addr=0.0.0.0".to_string(),
            "--http.port=8545".to_string(),
            "--http.api=eth,net,web3,debug,personal".to_string(),
            "--http.corsdomain=*".to_string(),
            "--http.vhosts=*".to_string(),
            "--ws".to_string(),
            "--ws.addr=0.0.0.0".to_string(),
            "--ws.port=8546".to_string(),
            "--ws.api=eth,net,web3".to_string(),
        ]
    }

    fn expose_ports(&self) -> &[u16] {
        &[8545, 8546]
    }
}
```

#### Nitro Sequencer

```rust
const NITRO_IMAGE: &str = "offchainlabs/nitro-node";
const NITRO_TAG: &str = "v3.2.1-d81324d"; // pin to specific release

pub struct NitroSequencer {
    l1_url: String,
    chain_config: PathBuf,  // mounted as volume
}

impl NitroSequencer {
    pub fn new(l1_url: String, chain_config: PathBuf) -> Self {
        Self { l1_url, chain_config }
    }
}

impl Image for NitroSequencer {
    fn name(&self) -> &str {
        NITRO_IMAGE
    }

    fn tag(&self) -> &str {
        NITRO_TAG
    }

    fn ready_conditions(&self) -> Vec<WaitFor> {
        vec![WaitFor::message_on_stderr("Running Arbitrum nitro")]
    }

    fn cmd(&self) -> impl IntoIterator<Item = impl Into<String>> {
        vec![
            "--node.dangerous.no-l1-listener".to_string(),
            "--node.sequencer".to_string(),
            "--execution.sequencer.enable".to_string(),
            "--node.feed.output.enable".to_string(),
            "--node.feed.output.port=9642".to_string(),
            "--http.addr=0.0.0.0".to_string(),
            "--http.port=8547".to_string(),
            "--http.api=eth,net,web3,arb,debug".to_string(),
            "--http.corsdomain=*".to_string(),
            "--http.vhosts=*".to_string(),
            "--ws.addr=0.0.0.0".to_string(),
            "--ws.port=8548".to_string(),
            format!("--parent-chain.connection.url={}", self.l1_url),
            "--chain.dev-init".to_string(),
            "--metrics".to_string(),
            "--metrics.addr=0.0.0.0".to_string(),
            "--metrics.port=6070".to_string(),
        ]
    }

    fn expose_ports(&self) -> &[u16] {
        &[8547, 8548, 9642, 6070]
    }
}
```

#### Feed Relay

```rust
pub struct FeedRelay {
    sequencer_feed_url: String,
}

impl Image for FeedRelay {
    fn name(&self) -> &str {
        NITRO_IMAGE
    }

    fn tag(&self) -> &str {
        NITRO_TAG
    }

    fn ready_conditions(&self) -> Vec<WaitFor> {
        vec![WaitFor::message_on_stderr("Feed relay started")]
    }

    fn cmd(&self) -> impl IntoIterator<Item = impl Into<String>> {
        vec![
            "relay".to_string(),
            "--node.feed.input.url".to_string(),
            self.sequencer_feed_url.clone(),
            "--node.feed.output.port=9642".to_string(),
            "--node.feed.output.addr=0.0.0.0".to_string(),
        ]
    }

    fn expose_ports(&self) -> &[u16] {
        &[9642]
    }
}
```

---

## 4. NitroDevnet Orchestrator

### 4.1 Lifecycle

```rust
pub struct NitroDevnet {
    pub l1_geth: ContainerAsync<L1Geth>,
    pub sequencer: ContainerAsync<NitroSequencer>,
    pub feed_relay: ContainerAsync<FeedRelay>,
    pub validator: Option<ContainerAsync<NitroValidator>>,

    pub l1_rpc_url: String,
    pub sequencer_rpc_url: String,
    pub sequencer_ws_url: String,
    pub feed_relay_url: String,
    pub prometheus_url: String,

    pub chain_id: u64,
    pub funded_accounts: Vec<FundedAccount>,

    _network: DockerNetwork,
}

pub struct FundedAccount {
    pub address: Address,
    pub private_key: SigningKey,
    pub balance: U256,
}

impl NitroDevnet {
    /// Boot a minimal devnet: L1 + Sequencer + Feed Relay
    pub async fn minimal() -> Result<Self, DevnetError> {
        Self::boot(DevnetProfile::Minimal).await
    }

    /// Boot a standard devnet: Minimal + Validator
    pub async fn standard() -> Result<Self, DevnetError> {
        Self::boot(DevnetProfile::Standard).await
    }

    /// Boot the full topology
    pub async fn full() -> Result<Self, DevnetError> {
        Self::boot(DevnetProfile::Full).await
    }

    async fn boot(profile: DevnetProfile) -> Result<Self, DevnetError> {
        let docker = testcontainers::clients::Cli::default();

        // 1. Create isolated Docker network
        let network = docker.create_network("nitro-devnet")?;

        // 2. Start L1 Geth
        let l1 = docker.run_in_network(L1Geth::new(), &network).await?;
        let l1_rpc = format!("http://localhost:{}", l1.get_host_port(8545).await?);
        Self::wait_for_l1(&l1_rpc).await?;

        // 3. Deploy L1 rollup contracts
        let rollup_config = Self::deploy_rollup_contracts(&l1_rpc).await?;

        // 4. Start Sequencer
        let sequencer = docker.run_in_network(
            NitroSequencer::new(l1.internal_url(8545), rollup_config),
            &network,
        ).await?;
        let seq_rpc = format!("http://localhost:{}", sequencer.get_host_port(8547).await?);
        Self::wait_for_sequencer(&seq_rpc).await?;

        // 5. Start Feed Relay
        let feed = docker.run_in_network(
            FeedRelay::new(sequencer.internal_url(9642)),
            &network,
        ).await?;
        let feed_url = format!("ws://localhost:{}", feed.get_host_port(9642).await?);

        // 6. Pre-fund test accounts
        let accounts = Self::fund_accounts(&seq_rpc, 100).await?;

        // 7. Optionally start validator
        let validator = if profile >= DevnetProfile::Standard {
            Some(Self::start_validator(&docker, &network, &l1, &rollup_config).await?)
        } else {
            None
        };

        Ok(Self {
            l1_geth: l1,
            sequencer,
            feed_relay: feed,
            validator,
            l1_rpc_url: l1_rpc,
            sequencer_rpc_url: seq_rpc,
            sequencer_ws_url: format!("ws://localhost:{}", sequencer.get_host_port(8548).await?),
            feed_relay_url: feed_url,
            prometheus_url: format!("http://localhost:{}/debug/metrics", sequencer.get_host_port(6070).await?),
            chain_id: 412346, // Nitro devnet default
            funded_accounts: accounts,
            _network: network,
        })
    }
}

// NitroDevnet implements Drop → containers are cleaned up automatically
```

### 4.2 Boot Sequence Detail

```text
Time 0s:   Start L1 Geth in dev mode (instant mining)
Time 2s:   L1 ready → deploy SequencerInbox, RollupCore, Bridge contracts
Time 8s:   L1 contracts deployed → start Sequencer with L1 pointers
Time 15s:  Sequencer ready → start Feed Relay pointing to sequencer feed
Time 18s:  Feed Relay ready → pre-fund 100 test accounts (batch tx)
Time 20s:  Devnet ready → return NitroDevnet handle to test
```

Target: **<30 seconds** for minimal profile.

### 4.3 Health Checks

Each container has specific readiness checks beyond Docker health:

```rust
impl NitroDevnet {
    async fn wait_for_l1(url: &str) -> Result<(), DevnetError> {
        Self::poll_until(|| async {
            let resp = reqwest::get(format!("{url}"))
                .await?
                .json::<serde_json::Value>()
                .await;
            // eth_chainId must return successfully
            let provider = alloy::providers::ProviderBuilder::new()
                .on_http(url.parse()?);
            provider.get_chain_id().await.map(|_| ())
        }, Duration::from_secs(30), "L1 Geth").await
    }

    async fn wait_for_sequencer(url: &str) -> Result<(), DevnetError> {
        Self::poll_until(|| async {
            let provider = alloy::providers::ProviderBuilder::new()
                .on_http(url.parse()?);
            let chain_id = provider.get_chain_id().await?;
            // Nitro devnet chain ID is not 1337 (that's L1)
            if chain_id == 1337 {
                return Err(anyhow!("got L1 chain ID, sequencer not ready"));
            }
            // Must be able to get a block
            provider.get_block_number().await.map(|_| ())
        }, Duration::from_secs(30), "Sequencer").await
    }

    async fn wait_for_feed(url: &str) -> Result<(), DevnetError> {
        Self::poll_until(|| async {
            // WebSocket handshake must succeed
            let (ws, _) = tokio_tungstenite::connect_async(url).await?;
            drop(ws);
            Ok(())
        }, Duration::from_secs(15), "Feed Relay").await
    }

    async fn poll_until<F, Fut>(
        check: F,
        timeout: Duration,
        component: &str,
    ) -> Result<(), DevnetError>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<(), anyhow::Error>>,
    {
        let start = Instant::now();
        loop {
            match check().await {
                Ok(()) => return Ok(()),
                Err(e) if start.elapsed() > timeout => {
                    return Err(DevnetError::HealthCheckTimeout {
                        component: component.to_string(),
                        elapsed: start.elapsed(),
                        last_error: e.to_string(),
                    });
                }
                Err(_) => tokio::time::sleep(Duration::from_millis(500)).await,
            }
        }
    }
}
```

---

## 5. Account Pre-Funding

### 5.1 Strategy

The Nitro devnet sequencer starts with a pre-funded "dev" account (well-known private key). We use that account to fund test accounts:

```rust
impl NitroDevnet {
    /// Pre-fund N test accounts with 100 ETH each
    async fn fund_accounts(
        sequencer_rpc: &str,
        count: usize,
    ) -> Result<Vec<FundedAccount>, DevnetError> {
        let provider = alloy::providers::ProviderBuilder::new()
            .on_http(sequencer_rpc.parse()?);

        // Nitro devnet genesis account
        let dev_key = SigningKey::from_bytes(
            &hex::decode(NITRO_DEV_PRIVATE_KEY)?
        )?;
        let dev_wallet = alloy::signers::local::PrivateKeySigner::from(dev_key);
        let funder = provider.with_signer(dev_wallet);

        let mut accounts = Vec::with_capacity(count);
        let funding_amount = alloy::primitives::utils::parse_ether("100")?;

        // Generate deterministic test accounts from a seed
        let mut rng = rand::rngs::StdRng::seed_from_u64(0xGH_TEST_SEED);

        for i in 0..count {
            let key = SigningKey::random(&mut rng);
            let address = alloy::signers::local::PrivateKeySigner::from(&key).address();

            // Send funding tx
            let tx = TransactionRequest::default()
                .to(address)
                .value(funding_amount);
            funder.send_transaction(tx).await?.watch().await?;

            accounts.push(FundedAccount {
                address,
                private_key: key,
                balance: funding_amount,
            });
        }

        tracing::info!(
            count = accounts.len(),
            each_balance = %funding_amount,
            "Pre-funded test accounts"
        );

        Ok(accounts)
    }
}

// Nitro devnet well-known dev key (public knowledge, devnet only)
const NITRO_DEV_PRIVATE_KEY: &str =
    "b6b15c8cb491557369f3c7d2c287b053eb229daa9c22138887752191c9520659";
```

### 5.2 Account Pool Integration

The `FundedAccount` list maps directly to the workload engine's account pool (RFC-0005 §6). In tests:

```rust
let devnet = NitroDevnet::minimal().await?;

// Create edge config pointing at the devnet
let edge_config = EdgeConfig {
    sequencer_rpc: devnet.sequencer_rpc_url.clone(),
    gateway_rpc: devnet.sequencer_rpc_url.clone(), // same in devnet
    feed_relay: devnet.feed_relay_url.clone(),
    account_pool: devnet.funded_accounts.iter()
        .map(|a| a.private_key.clone())
        .collect(),
    ..Default::default()
};
```

---

## 6. Test Categories

### 6.1 Test Matrix

| Category | Profile | What's Tested | Runs In |
| --- | --- | --- | --- |
| **Smoke** | Minimal | Devnet boots, RPC responds, feed connects | Every PR |
| **Transaction** | Minimal | Tx submission, receipt, gas usage, reverts | Every PR |
| **Feed** | Minimal | Feed subscription, message parsing, correlation | Every PR |
| **Workload** | Minimal | Template execution, rate limiting, gas modeling | Every PR |
| **Telemetry** | Minimal | Event generation, shipping, Parquet write | Every PR |
| **Batch Posting** | Standard | Batch poster lag, L1 data availability | Nightly |
| **Correctness** | Standard | Oracle checks, balance conservation, ordering | Nightly |
| **Fault Injection** | Standard | Network faults, feed disconnect, recovery | Nightly |
| **Full Lifecycle** | Standard | Hive→Edge→Run→Report end-to-end | Nightly |
| **Validator** | Full | Validator lag monitoring, BoLD assertions | Release gate |
| **Multi-Edge** | Standard | Multiple edge containers coordinated by hive | Release gate |

### 6.2 Test Organization

```text
tests/
├── integration/
│   ├── smoke.rs              # devnet boot + basic connectivity
│   ├── tx_submission.rs      # send txs, get receipts
│   ├── feed_consumer.rs      # subscribe to feed, parse messages
│   ├── workload_engine.rs    # execute workload templates against real chain
│   ├── telemetry.rs          # event generation and shipping
│   ├── oracle.rs             # correctness checks against real state
│   ├── fault_injection.rs    # inject faults, verify recovery
│   └── full_lifecycle.rs     # hive + edges + run + report
├── e2e/
│   ├── multi_edge.rs         # multiple edge containers
│   ├── batch_poster.rs       # L1 batch verification
│   ├── validator.rs          # validator lag and assertions
│   └── regression.rs         # regression detection against baseline
└── fixtures/
    ├── scenarios/            # test scenario YAML files
    ├── abis/                 # contract ABIs for test templates
    └── contracts/            # Solidity test contracts (compiled)
```

### 6.3 Test Fixtures

Deploy purpose-built test contracts during devnet setup:

```rust
pub struct TestContracts {
    pub counter: Address,        // simple storage: increment, read
    pub erc20: Address,          // standard ERC-20 for transfer tests
    pub gas_burner: Address,     // configurable compute-heavy contract
    pub storage_writer: Address, // writes to N storage slots
    pub reverter: Address,       // always reverts (for error handling tests)
    pub event_emitter: Address,  // emits logs for oracle testing
}

impl NitroDevnet {
    pub async fn deploy_test_contracts(&self) -> Result<TestContracts, DevnetError> {
        // Deploy from pre-compiled bytecode (included as const byte arrays)
        // No Solidity compiler needed at test time
        let deployer = self.funded_accounts[0].signer();

        let counter = deploy(&deployer, COUNTER_BYTECODE).await?;
        let erc20 = deploy(&deployer, ERC20_BYTECODE).await?;
        // ... etc

        Ok(TestContracts { counter, erc20, /* ... */ })
    }
}
```

Solidity source lives in `contracts/test/` and is compiled during `cargo xtask build-contracts`. The bytecode is embedded as `const` byte arrays — no Solidity toolchain needed to run tests.

---

## 7. Example Tests

### 7.1 Smoke Test

```rust
#[tokio::test]
async fn devnet_boots_and_responds() {
    let devnet = NitroDevnet::minimal().await.expect("devnet should boot");

    // Sequencer RPC responds
    let provider = ProviderBuilder::new()
        .on_http(devnet.sequencer_rpc_url.parse().unwrap());
    let chain_id = provider.get_chain_id().await.unwrap();
    assert_ne!(chain_id, 1337, "should not be L1 chain ID");

    // Feed relay accepts WebSocket connections
    let (ws, _) = tokio_tungstenite::connect_async(&devnet.feed_relay_url)
        .await
        .expect("feed relay should accept connections");
    drop(ws);

    // Funded accounts have balance
    let balance = provider
        .get_balance(devnet.funded_accounts[0].address)
        .await
        .unwrap();
    assert!(balance > U256::ZERO, "accounts should be funded");
}
```

### 7.2 Transaction Submission Test

```rust
#[tokio::test]
async fn submit_tx_and_get_receipt() {
    let devnet = NitroDevnet::minimal().await.unwrap();
    let contracts = devnet.deploy_test_contracts().await.unwrap();

    let signer = devnet.funded_accounts[0].signer();
    let provider = ProviderBuilder::new()
        .wallet(signer)
        .on_http(devnet.sequencer_rpc_url.parse().unwrap());

    // Submit a simple counter increment
    let call = ICounter::incrementCall {};
    let tx = TransactionRequest::default()
        .to(contracts.counter)
        .input(call.abi_encode().into());

    let pending = provider.send_transaction(tx).await.unwrap();
    let receipt = pending.get_receipt().await.unwrap();

    assert!(receipt.status(), "tx should succeed");
    assert!(receipt.gas_used > 0, "should consume gas");

    // Verify state changed
    let count = provider
        .call(
            &TransactionRequest::default()
                .to(contracts.counter)
                .input(ICounter::countCall {}.abi_encode().into()),
        )
        .await
        .unwrap();
    assert_eq!(U256::from_be_slice(&count), U256::from(1));
}
```

### 7.3 Feed Correlation Test

```rust
#[tokio::test]
async fn submitted_tx_appears_in_feed() {
    let devnet = NitroDevnet::minimal().await.unwrap();

    // Start feed consumer
    let (mut feed_rx, _handle) = FeedConsumer::connect(&devnet.feed_relay_url)
        .await
        .unwrap();

    // Submit a transaction
    let signer = devnet.funded_accounts[0].signer();
    let provider = ProviderBuilder::new()
        .wallet(signer)
        .on_http(devnet.sequencer_rpc_url.parse().unwrap());

    let tx_hash = provider
        .send_transaction(TransactionRequest::default()
            .to(Address::ZERO)
            .value(U256::from(1)))
        .await
        .unwrap()
        .tx_hash();

    // Feed should contain our tx within 5 seconds
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut found = false;
    while Instant::now() < deadline {
        if let Ok(msg) = tokio::time::timeout(
            Duration::from_secs(1),
            feed_rx.recv(),
        ).await {
            if let Some(feed_msg) = msg {
                if feed_msg.contains_tx(tx_hash) {
                    found = true;
                    break;
                }
            }
        }
    }
    assert!(found, "tx should appear in feed within 5s");
}
```

### 7.4 Workload Engine Integration Test

```rust
#[tokio::test]
async fn workload_engine_executes_against_real_chain() {
    let devnet = NitroDevnet::minimal().await.unwrap();
    let contracts = devnet.deploy_test_contracts().await.unwrap();

    let engine = WorkloadEngine::new(WorkloadConfig {
        target_gas_rate: GasRate::Sustained(5_000_000), // 5M gas/sec
        profiles: vec![
            WeightedTemplate {
                template: Template::Erc20Transfer {
                    token: contracts.erc20,
                },
                weight: 50,
            },
            WeightedTemplate {
                template: Template::CounterIncrement {
                    contract: contracts.counter,
                },
                weight: 50,
            },
        ],
        account_pool: devnet.funded_accounts.clone(),
        seed: 42,
        ..Default::default()
    });

    // Run for 10 seconds
    let (tx_sender, mut rx) = tokio::sync::mpsc::channel(1000);
    let handle = tokio::spawn(async move {
        engine.run(tx_sender, Duration::from_secs(10)).await
    });

    let mut tx_count = 0u64;
    while let Some(event) = rx.recv().await {
        tx_count += 1;
    }

    handle.await.unwrap().unwrap();

    assert!(tx_count > 0, "should have generated transactions");
    tracing::info!(tx_count, "workload engine generated txs against real devnet");
}
```

### 7.5 Correctness Oracle Test

```rust
#[tokio::test]
async fn balance_conservation_holds_after_workload() {
    let devnet = NitroDevnet::minimal().await.unwrap();

    // Record initial balances
    let provider = ProviderBuilder::new()
        .on_http(devnet.sequencer_rpc_url.parse().unwrap());
    let initial_balances: Vec<U256> = futures::future::join_all(
        devnet.funded_accounts.iter()
            .map(|a| provider.get_balance(a.address))
    ).await.into_iter().collect::<Result<_, _>>().unwrap();

    // Run a short workload (transfers between our accounts)
    run_workload(&devnet, Duration::from_secs(5)).await.unwrap();

    // Run the balance conservation oracle check
    let final_balances: Vec<U256> = futures::future::join_all(
        devnet.funded_accounts.iter()
            .map(|a| provider.get_balance(a.address))
    ).await.into_iter().collect::<Result<_, _>>().unwrap();

    // Total should be conserved (minus gas spent)
    let initial_total: U256 = initial_balances.iter().sum();
    let final_total: U256 = final_balances.iter().sum();

    // final_total should be <= initial_total (gas was spent)
    // but the difference should equal total gas cost
    assert!(
        final_total <= initial_total,
        "balance should not increase"
    );

    let gas_spent = initial_total - final_total;
    tracing::info!(%gas_spent, "balance conservation verified");
}
```

---

## 8. CI Integration

### 8.1 GitHub Actions

```yaml
name: integration-tests
on:
  push:
    branches: [main]
  pull_request:

jobs:
  integration:
    runs-on: ubuntu-latest
    services:
      docker:
        image: docker:dind
        options: --privileged
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Cache Docker images
        uses: ScribeMD/docker-cache@0.5.0
        with:
          key: docker-${{ runner.os }}-nitro

      - name: Run integration tests (minimal profile)
        run: cargo test --test integration -- --test-threads=1
        env:
          TESTCONTAINERS_COMMAND: keep
          RUST_LOG: gashammer=debug

  nightly:
    runs-on: ubuntu-latest
    if: github.event_name == 'schedule'
    steps:
      - uses: actions/checkout@v4
      - name: Run full E2E suite
        run: cargo test --test e2e -- --test-threads=1
        env:
          GASHAMMER_TEST_PROFILE: standard
```

### 8.2 Image Caching

Nitro Docker images are large (~500MB). Caching strategy:

1. **GitHub Actions cache:** Cache Docker image layers between runs
2. **Pre-pull in CI setup step:** `docker pull offchainlabs/nitro-node:v3.2.1-d81324d`
3. **Pin image tags:** Never use `latest` in CI; pin to specific Nitro releases
4. **Local dev:** First run pulls images; subsequent runs use cached images

### 8.3 Test Parallelism

Testcontainers tests use Docker resources (ports, networks). To avoid conflicts:

- Integration tests run with `--test-threads=1` (sequential)
- Each test gets its own Docker network with unique name
- Port mapping is dynamic (testcontainers handles this automatically)
- For faster CI: use a matrix strategy with different test categories on separate runners

---

## 9. Local Development Workflow

### 9.1 Prerequisites

```bash
# Only requirement: Docker
docker --version  # must be running

# Optional: pre-pull images to speed up first run
docker pull ethereum/client-go:stable
docker pull offchainlabs/nitro-node:v3.2.1-d81324d
```

### 9.2 Running Tests

```bash
# Run all integration tests
cargo test --test integration

# Run a specific test
cargo test --test integration tx_submission

# Run with logging
RUST_LOG=gashammer=debug cargo test --test integration -- --nocapture

# Run E2E suite (slower, needs standard profile)
GASHAMMER_TEST_PROFILE=standard cargo test --test e2e

# Keep containers running after test (for debugging)
TESTCONTAINERS_COMMAND=keep cargo test --test integration smoke
# Then inspect: docker ps, docker logs <container>
```

### 9.3 Debugging Failed Tests

When a test fails, the devnet orchestrator captures diagnostics:

```rust
impl NitroDevnet {
    pub async fn dump_diagnostics(&self) -> DiagnosticsBundle {
        DiagnosticsBundle {
            l1_logs: self.l1_geth.logs().await,
            sequencer_logs: self.sequencer.logs().await,
            feed_logs: self.feed_relay.logs().await,
            l1_block_number: self.l1_block_number().await,
            l2_block_number: self.l2_block_number().await,
            sequencer_metrics: self.scrape_prometheus().await,
        }
    }
}
```

On test failure, diagnostics are written to `target/test-diagnostics/{test_name}/`.

---

## 10. Contract Test Harness

### 10.1 Test Contracts

Purpose-built Solidity contracts for testing specific GasHammer behaviors:

| Contract | Purpose | Key Functions |
| --- | --- | --- |
| `Counter.sol` | Simple state mutation | `increment()`, `count()` |
| `GasHammerERC20.sol` | ERC-20 token for transfer tests | Standard ERC-20 + mint |
| `GasBurner.sol` | Configurable compute consumption | `burn(uint256 gasAmount)` |
| `StorageWriter.sol` | Writes to N storage slots | `writeSlots(uint256 count)` |
| `Reverter.sol` | Deterministic reverts | `revertIf(bool condition)` |
| `EventEmitter.sol` | Emits events for oracle testing | `emit(bytes32 topic, bytes data)` |
| `ContentionTarget.sol` | Shared state for hot-slot tests | `compete(uint256 key)` |

### 10.2 Build Pipeline

```text
contracts/test/
├── Counter.sol
├── GasHammerERC20.sol
├── GasBurner.sol
├── ...
└── build/
    ├── Counter.json          # ABI + bytecode (committed)
    └── ...
```

Built via `cargo xtask build-contracts` using `solc` or `forge build`. The compiled artifacts (ABI + bytecode JSON) are committed to the repo so that `cargo test` never needs a Solidity toolchain.

---

## 11. Performance Benchmarks as Tests

### 11.1 Throughput Baseline

Integration tests establish and enforce throughput baselines:

```rust
#[tokio::test]
async fn sequencer_accepts_at_least_1000_tps() {
    let devnet = NitroDevnet::minimal().await.unwrap();

    let start = Instant::now();
    let mut count = 0u64;

    // Blast simple transfers for 10 seconds
    while start.elapsed() < Duration::from_secs(10) {
        let account = &devnet.funded_accounts[count as usize % devnet.funded_accounts.len()];
        let tx = TransactionRequest::default()
            .to(Address::ZERO)
            .value(U256::from(1));

        // Fire-and-forget (don't wait for receipt)
        provider.send_transaction(tx).await.unwrap();
        count += 1;
    }

    let tps = count as f64 / 10.0;
    tracing::info!(tps, "achieved transaction submission rate");

    // Devnet should handle at least 1000 TPS for simple transfers
    assert!(tps > 1000.0, "devnet TPS ({tps}) below minimum threshold");
}
```

### 11.2 Latency Baseline

```rust
#[tokio::test]
async fn accept_latency_under_100ms_on_devnet() {
    let devnet = NitroDevnet::minimal().await.unwrap();
    let mut latencies = Vec::new();

    for _ in 0..100 {
        let start = Instant::now();
        let pending = provider.send_transaction(simple_transfer()).await.unwrap();
        let _receipt = pending.get_receipt().await.unwrap();
        latencies.push(start.elapsed());
    }

    latencies.sort();
    let p50 = latencies[49];
    let p99 = latencies[98];

    tracing::info!(?p50, ?p99, "devnet accept latency");
    assert!(p99 < Duration::from_millis(100), "p99 latency too high on devnet");
}
```

---

## 12. Risks

### Risk: Nitro Docker images are large and slow to pull

The `offchainlabs/nitro-node` image is ~500MB.

**Mitigation:** Pin image tags (no `:latest` churn). Cache layers in CI. Pre-pull in setup step. Locally, first run is slow; subsequent runs use cached images.

### Risk: Nitro devnet behavior differs from production

Devnet uses instant L1 finality, no real gas market, no congestion.

**Mitigation:** Devnet tests validate *correctness*, not *performance characteristics under production conditions*. Performance testing against staging environments is a separate concern (RFC-0010 scenarios). The devnet is for: "does our code talk to Nitro correctly?"

### Risk: Testcontainers port conflicts in CI

Multiple test jobs or parallel tests may conflict on Docker resources.

**Mitigation:** Testcontainers assigns random host ports. Tests use unique Docker network names. Sequential test execution (`--test-threads=1`) for integration tests.

### Risk: L1 contract deployment is fragile

Nitro's L1 contracts have complex deployment scripts.

**Mitigation:** Use Nitro's own initialization (`--chain.dev-init` flag) which handles genesis and L1 setup automatically. Fall back to pre-deployed state snapshots if init flags are insufficient.

### Risk: Tests become slow as suite grows

More tests = longer CI times.

**Mitigation:** Test categories run in parallel on separate CI runners. Minimal profile for PR checks (~2 min). Standard/Full profiles for nightly only. Container reuse within a test file (one devnet boot, multiple test functions).

---

## 13. Open Questions

1. Should we support Nitro's `--chain.dev-init` for zero-config devnet setup, or deploy L1 contracts manually for more control?
2. Should the testenv crate be published to crates.io as a standalone tool for the Nitro ecosystem?
3. Should we add a `cargo xtask devnet` command that boots a persistent devnet for manual exploration?
4. How should we handle Nitro version upgrades — pin to a specific release and update periodically, or test against multiple versions?
5. Should test contracts be written in Solidity or Stylus (Rust-native smart contracts on Arbitrum)?
6. Should we create a `NitroDevnetPool` that pre-boots devnets and hands them out to tests for faster parallel execution?

---

## 14. References

- [Offchain Labs nitro-testnode](https://github.com/OffchainLabs/nitro-testnode)
- [Offchain Labs Nitro Docker images](https://hub.docker.com/r/offchainlabs/nitro-node)
- [testcontainers-rs](https://github.com/testcontainers/testcontainers-rs)
- [Nitro --chain.dev-init documentation](https://docs.arbitrum.io/run-arbitrum-node/run-local-dev-node)
- RFC-0001 §2–5 (Nitro protocols — what we're testing against)
- RFC-0005 §2 (Transaction templates — what gets executed)
- RFC-0007 §3–4 (Correctness checks — what gets validated)
