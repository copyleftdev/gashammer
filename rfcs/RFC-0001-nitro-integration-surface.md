# RFC-0001: Nitro Integration Surface

## Status

Draft

## Author

Don Johnson

## Summary

This RFC defines every concrete protocol, endpoint, data structure, and observation point where GasHammer interacts with the Arbitrum Nitro stack. It serves as the foundational reference for all subsequent RFCs. No GasHammer component should invent an integration path not documented here.

---

## 1. Scope

GasHammer is an external harness. It does **not** modify, fork, or link against Nitro node internals. All integration occurs through:

1. **Wire protocols** — JSON-RPC over HTTP, WebSocket feeds
2. **On-chain contracts** — SequencerInbox, RollupCore, ArbOS precompiles
3. **Node operator APIs** — Debug/admin RPC namespaces (when available on test targets)
4. **Infrastructure telemetry** — Prometheus metrics endpoints exposed by Nitro nodes

This boundary is non-negotiable. GasHammer must never require a patched Nitro node to function. Optional enrichment from debug endpoints is acceptable but must degrade gracefully.

---

## 2. Nitro Architecture — Components Under Test

### 2.1 Component Map

```
                          ┌─────────────────────────────┐
                          │        Ethereum L1           │
                          │  ┌───────────┐ ┌──────────┐ │
                          │  │ Sequencer │ │ Rollup   │ │
                          │  │ Inbox     │ │ Core     │ │
                          │  └───────────┘ └──────────┘ │
                          └──────┬──────────────┬───────┘
                                 │              │
                    batch posting│     assertions│
                                 │              │
┌────────────────────────────────┼──────────────┼──────────────┐
│  Arbitrum L2 (Nitro)          │              │              │
│                                │              │              │
│  ┌──────────┐  ┌───────────┐  │  ┌─────────┐ │              │
│  │ RPC      │  │ Sequencer │──┘  │Validator│─┘              │
│  │ Gateway  │  │           │     │ (BoLD)  │                │
│  └────┬─────┘  └─────┬─────┘     └────┬────┘                │
│       │              │                 │                     │
│       │         ┌────┴─────┐     ┌─────┴──────┐             │
│       │         │ Feed     │     │ Batch      │             │
│       │         │ Relay    │     │ Poster     │             │
│       │         │ (WS:9642)│     └────────────┘             │
│       │         └──────────┘                                │
│       │                                                     │
│  ┌────┴──────────────────────────────────┐                  │
│  │  STF: Geth Core → ArbOS → Node API   │                  │
│  └───────────────────────────────────────┘                  │
└─────────────────────────────────────────────────────────────┘
```

### 2.2 Component Responsibilities (GasHammer Perspective)

| Component | What GasHammer Does With It |
|---|---|
| **Sequencer** | Primary tx submission target; measures ordering latency, fairness, Timeboost behavior |
| **Feed Relay** | Subscribe for soft finality timestamps; detect feed lag and disconnection |
| **Batch Poster** | Observe L1 posting behavior; measure batch age, size, compression, cost |
| **Validator (BoLD)** | Measure replay lag; track assertion submission frequency |
| **RPC Gateway** | General-purpose queries; receipt polling; gas price monitoring; state reads |
| **STF (Geth + ArbOS)** | Indirectly observed via execution receipts, gas consumption, state growth |
| **SequencerInbox (L1)** | Monitor batch postings; test delayed inbox submission path |
| **RollupCore (L1)** | Monitor assertion chain; validator health |
| **ArbOS Precompiles** | Generate specialized workloads exercising L2-specific features |

---

## 3. Integration Point: Transaction Submission

### 3.1 Primary Path — Sequencer Endpoint

**Protocol:** HTTP POST, JSON-RPC 2.0

**Endpoint pattern:** `https://<sequencer-host>/rpc`

Known public endpoints (for reference/testing):
- Arbitrum One: `https://arb1-sequencer.arbitrum.io/rpc`
- Arbitrum Nova: `https://nova-sequencer.arbitrum.io/rpc`
- Sepolia testnet: `https://sepolia-rollup-sequencer.arbitrum.io/rpc`

**Supported methods on sequencer endpoints:**
- `eth_sendRawTransaction`
- `eth_sendRawTransactionConditional`

**No other methods are guaranteed.** General queries must go through the RPC gateway, not the sequencer endpoint.

**Wire format:**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "eth_sendRawTransaction",
  "params": ["0x<rlp-encoded-signed-tx>"]
}
```

**Response (success):**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": "0x<tx-hash>"
}
```

**GasHammer measurement contract:**

| Timestamp | What | Clock |
|---|---|---|
| `t_send` | HTTP POST initiated | Monotonic |
| `t_ack` | HTTP 200 received with tx hash | Monotonic |
| `t_accept` | Tx appears in sequencer feed | Monotonic (feed subscription) |
| `t_include` | Receipt available via `eth_getTransactionReceipt` | Wall (RPC query) |
| `t_batch` | Batch containing tx posted to L1 | Wall (L1 block timestamp) |
| `t_finalize` | L1 block containing batch reaches finality | Wall (L1 finality) |

**Latency definitions:**
- **Submission latency** = `t_ack - t_send`
- **Acceptance latency** = `t_accept - t_send`
- **Inclusion latency** = `t_include - t_send`
- **Batch latency** = `t_batch - t_send`
- **Finality latency** = `t_finalize - t_send`

### 3.2 Secondary Path — Delayed Inbox (Censorship Resistance)

**Protocol:** L1 Ethereum transaction to `SequencerInbox` contract

**Use cases in GasHammer:**
- Test force-inclusion path
- Measure delayed sequencer pickup time (~10 min normal, 24h force-inclusion deadline)
- Verify censorship resistance properties under sequencer overload

**Contract interaction:**
- Call `sendL2Message` on the `Inbox` contract (L1)
- Monitor delayed sequencer on L2 for inclusion

**This path is scenario-optional.** Most workloads use Path A.

### 3.3 Transaction Construction Requirements

GasHammer edges must construct valid Arbitrum transactions. Key differences from L1 Ethereum:

- **Chain ID:** Arbitrum One = 42161, Nova = 42170, Sepolia = 421614
- **Gas pricing:** Dual model — L2 computation gas + L1 data posting component
  - Use `eth_gasPrice` or EIP-1559 fields (`maxFeePerGas`, `maxPriorityFeePerGas`)
  - L1 data component is estimated by ArbOS and included in the gas charge
- **Gas limit estimation:** Must use `eth_estimateGas` against the target chain; L1 data costs inflate gas beyond pure EVM estimates
- **Nonce management:** Standard sequential nonces per account; GasHammer must maintain per-account nonce pools
- **Transaction types:** Support Type 0 (legacy), Type 2 (EIP-1559); Type 2 preferred

---

## 4. Integration Point: Sequencer Feed

### 4.1 Protocol

**Transport:** WebSocket  
**Default port:** 9642  
**Connection target:** Feed relay node (not sequencer directly)  
**Feed URL pattern:** `ws://<relay-host>:9642`

The feed relay is a separate process that subscribes to the sequencer and redistributes the uncompressed feed. In test environments, GasHammer should connect to a local or dedicated relay.

### 4.2 Message Structure

Top-level envelope:

```
BroadcastMessage {
    version:  int
    messages: [BroadcastFeedMessage]
    confirmedSequenceNumberMessage: ConfirmedSequenceNumberMessage (optional)
}
```

Per-message:

```
BroadcastFeedMessage {
    sequenceNumber: uint64          // Global ordering index
    message: MessageWithMetadata {
        message: L1IncomingMessage {
            header: L1IncomingMessageHeader {
                kind:        uint8   // 3 = sequencer message
                sender:      address // 0xa4b0...73657175656e636572 for sequencer
                blockNumber: uint64  // L1 block number (NOT L2)
                timestamp:   uint64  // L1 timestamp
                requestId:   bytes32 (nullable)
                baseFeeL1:   uint256 (nullable)
            }
            l2Msg: bytes            // Encoded L2 transaction(s)
            batchGasCost: uint64 (optional)
        }
        delayedMessagesRead: uint64
    }
    signature:     bytes (nullable)
    blockMetadata: bytes
}
```

### 4.3 Deriving L2 Block Number from Feed

- **Arbitrum One:** `l2_block = sequenceNumber + 22207817` (genesis offset)
- **Arbitrum Nova:** `l2_block = sequenceNumber` (genesis = 0)
- **Custom chains:** Genesis offset is chain-specific

### 4.4 GasHammer Feed Consumer Requirements

1. Maintain a persistent WebSocket connection per edge
2. Parse `BroadcastFeedMessage` and extract `sequenceNumber`, `timestamp`, and `l2Msg`
3. Decode `l2Msg` to extract transaction hashes for correlation with submitted transactions
4. Record `t_accept` (monotonic time of message receipt) for each correlated tx
5. Track `confirmedSequenceNumberMessage` for batch confirmation signals
6. Handle reconnection with backfill from last known sequence number
7. Emit feed health metrics: message rate, gap detection, reconnection count

### 4.5 Feed Failure Modes GasHammer Must Handle

| Failure | Detection | Response |
|---|---|---|
| Feed disconnect | WebSocket close/error | Reconnect with exponential backoff; log gap |
| Message gap | Sequence number discontinuity | Flag missing range; attempt backfill via RPC |
| Feed stall | No messages for >1s (at 250ms block time) | Emit stall metric; do not abort scenario unless threshold exceeded |
| Corrupt message | JSON parse failure | Log + skip; emit error metric |

---

## 5. Integration Point: RPC Gateway

### 5.1 Measurement Endpoints

| Purpose | Method | Frequency | Notes |
|---|---|---|---|
| **Receipt polling** | `eth_getTransactionReceipt` | Per submitted tx | Poll until non-null or timeout |
| **Chain head** | `eth_blockNumber` | Periodic (1-5s) | Track progression rate |
| **Gas price** | `eth_gasPrice` | Periodic (5-10s) | EIP-1559 fee dynamics under load |
| **Pending backlog** | `txpool_status` (if available) | Periodic (5s) | Requires debug namespace; degrade gracefully |
| **Block details** | `eth_getBlockByNumber` | Per block (selective) | Gas used, tx count, timestamps |
| **Nonce** | `eth_getTransactionCount` | Per account on init + on nonce error | Account pool management |
| **State reads** | `eth_call`, `eth_getStorageAt` | Oracle-driven | Correctness verification |
| **Logs** | `eth_getLogs` | Oracle-driven | Event correctness checks |
| **Debug trace** | `debug_traceTransaction` | Selective (if available) | Deep execution analysis; optional enrichment |

### 5.2 RPC Connection Strategy

- **Separate connection pools** for sequencer endpoint vs RPC gateway — they are different services
- **Rate limiting awareness** — public endpoints have rate limits; GasHammer test targets should be dedicated nodes
- **Connection pooling** — maintain persistent HTTP connections with keep-alive
- **Timeout policy** — configurable per-method; receipt polling should use a longer timeout than submission

### 5.3 Important Limitation

**Arbitrum public RPCs do not support WebSocket.** WebSocket is only available for the sequencer feed (port 9642 via relay). All RPC queries are HTTP-only against public infrastructure. Dedicated nodes may expose WebSocket RPC, but GasHammer must not depend on it.

---

## 6. Integration Point: Batch Poster Observation

### 6.1 What to Observe

The batch poster is an internal Nitro component that:
1. Accumulates sequenced transactions
2. Compresses with Brotli (levels 0-11, adaptive based on congestion)
3. Posts to L1 `SequencerInbox` contract via EIP-4844 blobs (primary) or calldata (fallback)

### 6.2 Observation Methods

**Method A: L1 Contract Events**

Monitor `SequencerInbox` contract on L1 for `SequencerBatchDelivered` events:

```
event SequencerBatchDelivered(
    uint256 indexed batchSequenceNumber,
    bytes32 indexed beforeAcc,
    bytes32 indexed afterAcc,
    bytes32 delayedAcc,
    uint256 afterDelayedMessagesRead,
    TimeBounds timeBounds,
    BatchDataLocation dataLocation
)
```

This gives: batch sequence number, timing bounds, data location (blob vs calldata).

**Method B: Nitro Node Metrics (if accessible)**

Nitro nodes expose Prometheus metrics at `/debug/metrics` or similar. Relevant batch poster metrics include:
- Batch size
- Posting frequency
- Compression ratio
- L1 gas spent

**Method A is always available.** Method B requires access to the node's metrics endpoint (typical in staging/test environments).

### 6.3 Derived Metrics

| Metric | Derivation |
|---|---|
| **Batch age** | L1 block timestamp of posting − timestamp of earliest tx in batch |
| **Batch size** | Decoded from blob/calldata; or inferred from sequence number range |
| **Posting frequency** | Time between consecutive `SequencerBatchDelivered` events |
| **L1 cost per batch** | L1 tx gas used × gas price (for calldata); blob gas × blob gas price (for blobs) |
| **Data location mode** | `BatchDataLocation` enum from event (0 = calldata, ? = blob) |

---

## 7. Integration Point: Validator / BoLD

### 7.1 What "Validator Lag" Means Concretely

The PRD uses the term "validator lag." In Nitro, this maps to:

- **Sync lag:** Difference between validator node's latest processed block and sequencer head
- **Assertion lag:** Time between a batch being posted and a validator submitting an assertion covering it
- **Replay throughput:** Rate at which the validator's STF re-executes blocks (gas/sec or blocks/sec)

### 7.2 Observation Methods

**Method A: L1 RollupCore Contract**

Monitor `RollupCore` contract for assertion events:

```
event AssertionCreated(bytes32 indexed assertionHash)
event AssertionConfirmed(bytes32 indexed assertionHash)
```

Track assertion creation frequency, confirmation delay, and any challenge initiations.

**Method B: Validator Node RPC (if accessible)**

Query the validator node's `eth_blockNumber` and compare to sequencer head to compute sync lag.

**Method C: Nitro Node Metrics (if accessible)**

Prometheus metrics from the validator node for replay throughput, queue depths.

### 7.3 BoLD-Specific Considerations

BoLD (Bounded Liquidity Delay) is the current dispute protocol:
- Permissionless validation — anyone can validate
- Multi-round challenge games narrowing to a single-step proof (OSP)
- Challenge period: ~6.4 days for assertion confirmation

**GasHammer's role:** Measure validator performance under load, not dispute correctness. GasHammer should track:
- Whether assertions keep pace with batch postings
- Whether replay throughput degrades under heavy L2 load
- Whether validator nodes experience resource exhaustion

---

## 8. Integration Point: ArbOS Precompiles

ArbOS precompiles are contracts at fixed addresses that expose L2-specific functionality. GasHammer workloads should exercise these to test ArbOS behavior under load.

### 8.1 Relevant Precompiles

| Precompile | Address | GasHammer Use Case |
|---|---|---|
| `ArbSys` | `0x0000...0064` | L2-to-L1 messaging; block/tx metadata queries |
| `ArbGasInfo` | `0x0000...006C` | Gas price queries under load; fee component breakdown |
| `ArbRetryableTx` | `0x0000...006E` | Retryable ticket creation/redemption under contention |
| `ArbAggregator` | `0x0000...006D` | Aggregator behavior |
| `ArbStatistics` | `0x0000...006F` | Chain statistics |
| `NodeInterface` | `0x00000000000000000000000000000000000000C8` | Gas estimation helpers; L1 cost estimation |

### 8.2 Workload Classes That Use Precompiles

- **Cross-chain messaging load:** Repeated `ArbSys.sendTxToL1()` calls to stress L2→L1 message path
- **Retryable ticket storms:** Bulk retryable creation to test ArbOS ticket management
- **Gas estimation stress:** Concurrent `NodeInterface.estimateRetryableTicket()` calls

---

## 9. Integration Point: Gas and Fee Model

### 9.1 Dual Fee Structure

Arbitrum charges two fee components:

1. **L2 computation gas** — Standard EVM gas priced via EIP-1559 dynamic pricing. The chain has a gas target; exceeding it triggers base fee escalation.

2. **L1 data posting cost** — Estimated by ArbOS based on transaction size and current L1 gas/blob prices. Charged as additional gas units.

### 9.2 Implications for GasHammer

- **Gas-weighted workload profiles** (PRD Section 8.1) must account for both components
- A "heavy" transaction in gas terms may be heavy because of computation (complex contract call) OR because of calldata size (large input data → high L1 posting cost)
- Under high load, EIP-1559 fee escalation changes the cost of submitting transactions — GasHammer must track `baseFee` progression as a load indicator
- **Gas target** is the key parameter: sustained gas consumption above target triggers fee escalation

### 9.3 Fee Observation

| Metric | Source |
|---|---|
| L2 base fee | `eth_gasPrice` or block header `baseFee` |
| L1 base fee component | `ArbGasInfo.getL1BaseFeeEstimate()` precompile |
| L1 data price | `ArbGasInfo.getL1GasPriceEstimate()` precompile |
| Per-tx gas breakdown | Transaction receipt `gasUsed` + `effectiveGasPrice` |
| Fee escalation rate | Delta of `baseFee` across consecutive blocks |

---

## 10. Integration Point: Timeboost

### 10.1 What It Is

Timeboost is an MEV auction mechanism for Arbitrum chains:
- Express lane auction every 60 seconds
- Winner gets priority transaction inclusion for the round
- Non-express transactions are delayed by 200ms
- Block time remains 250ms

### 10.2 GasHammer Relevance

Timeboost is relevant to **ordering fairness** testing:
- Measure whether express-lane transactions consistently get priority
- Test ordering behavior under contention between express and non-express paths
- Stress the auction mechanism under high-frequency bidding

**This is a scenario-optional integration.** Not all target chains run Timeboost. GasHammer scenario definitions should declare whether Timeboost is enabled and configure workload accordingly.

---

## 11. Integration Point: AnyTrust / Data Availability Committee

### 11.1 What It Is

AnyTrust chains (e.g., Arbitrum Nova) use a Data Availability Committee (DAC) instead of posting full data to L1:
- Committee of N members; assumes ≥2 honest
- Batch poster sends data to committee, collects BLS-signed DACerts
- Posts compact DACert to L1 instead of full data
- Falls back to full L1 posting if committee is unavailable

### 11.2 GasHammer Relevance

For AnyTrust target chains, GasHammer should:
- Monitor DACert posting instead of blob/calldata posting
- Measure committee response latency
- Test fallback-to-L1 behavior under committee degradation
- Track data availability guarantees

**This is chain-type dependent.** Scenario definitions must specify whether the target is a Rollup chain (full L1 data) or AnyTrust chain (DAC).

---

## 12. Integration Point: Node Metrics (Prometheus)

### 12.1 Availability

Nitro nodes expose Prometheus-compatible metrics. In test/staging environments operated by the team, these should be accessible to GasHammer.

### 12.2 Key Metric Families

| Category | Example Metrics | GasHammer Use |
|---|---|---|
| **Sequencer** | tx queue depth, ordering rate, feed broadcast rate | Sequencer saturation detection |
| **Batch poster** | batch size, posting rate, compression time, L1 gas | Batch health |
| **Execution** | block gas used, execution time per block, state size | STF performance |
| **Validator** | replay rate, sync lag, assertion count | Validator health |
| **Network** | peer count, feed connections, RPC request rate | Infrastructure health |
| **System** | CPU, memory, disk I/O, goroutine count | Resource exhaustion detection |

### 12.3 Integration Model

GasHammer hive scrapes Prometheus endpoints of target nodes and correlates with its own telemetry timeline. This provides the "infra metrics" layer described in PRD Section 13.

**Fallback:** If Prometheus is unavailable (e.g., testing against public infrastructure), GasHammer operates with external-only metrics (RPC-derived + feed-derived). Degraded but functional.

---

## 13. Nitro Block Timing Model

### 13.1 Block Production

- **Default block time:** 250ms
- **Blocks are produced when transactions are available** — not on a fixed timer
- **Empty blocks** are not produced; block production pauses when the mempool is empty
- **Under load:** One block per 250ms with gas limit per block

### 13.2 Implications for GasHammer

- Measuring "blocks per second" is misleading — use "gas per second" as the primary throughput metric
- Block timestamps in Arbitrum are sequencer-assigned, not miner-assigned
- Time between blocks can vary (250ms under load, unbounded when idle)
- GasHammer must use its own monotonic timestamps, not block timestamps, for latency measurement

---

## 14. Summary of Integration Boundaries

| Boundary | Protocol | Auth | Required/Optional |
|---|---|---|---|
| Sequencer tx submission | HTTP JSON-RPC | None (public) or API key | **Required** |
| RPC gateway queries | HTTP JSON-RPC | None or API key | **Required** |
| Sequencer feed | WebSocket (port 9642) | None | **Required** |
| L1 contract monitoring | HTTP JSON-RPC (L1 node) | None or API key | **Required** for batch/validator metrics |
| Node Prometheus metrics | HTTP `/metrics` | Network access | **Optional** (enrichment) |
| Debug RPC namespace | HTTP JSON-RPC | Admin access | **Optional** (deep analysis) |
| Delayed Inbox submission | L1 Ethereum tx | Wallet/signer | **Optional** (scenario-specific) |
| AnyTrust DAC monitoring | DAC REST API | Network access | **Optional** (AnyTrust chains only) |

---

## 15. Risks

### Risk: Sequencer endpoint rate limits

Public sequencer endpoints are rate-limited. GasHammer at load-test scale **must** target dedicated infrastructure, not public endpoints.

**Mitigation:** Scenario definitions must specify target endpoints. GasHammer preflight should warn if targeting known public endpoints.

### Risk: Feed relay scalability

A single feed relay may not sustain connections from 100 edges.

**Mitigation:** Deploy multiple feed relays; edges distribute across relays. GasHammer hive should manage relay assignment.

### Risk: L1 monitoring lag

L1 block times (~12s) introduce inherent granularity in batch and assertion observations.

**Mitigation:** Document this resolution limit. Batch latency measurements have ±12s granularity from L1 block timing.

### Risk: Prometheus metric names change across Nitro versions

Metric names are not part of Nitro's stable API.

**Mitigation:** Abstract metric scraping behind a version-aware adapter. Degrade gracefully on missing metrics.

### Risk: ArbOS precompile gas costs change

Precompile gas schedules can change between Nitro releases, breaking gas-weighted workload profiles.

**Mitigation:** Workload profiles should re-estimate gas costs during preflight. Store gas estimates as part of the scenario artifact.

---

## 16. Open Questions

1. Should GasHammer maintain its own feed relay, or always depend on the target environment's relay infrastructure?
2. What Prometheus metric names are stable across Nitro releases? Is there a published metric contract?
3. Should Timeboost express-lane auction participation be a first-class workload type, or deferred post-MVP?
4. For AnyTrust chains, should GasHammer directly query individual DAC members, or only observe DACert postings on L1?
5. What is the preferred method for measuring validator replay throughput — direct node metrics or inference from assertion timing?

---

## 17. References

- [Inside Arbitrum Nitro](https://docs.arbitrum.io/how-arbitrum-works/inside-arbitrum-nitro)
- [Nitro source (OffchainLabs/nitro)](https://github.com/OffchainLabs/nitro)
- [Sequencer feed documentation](https://docs.arbitrum.io/run-arbitrum-node/sequencer/read-sequencer-feed)
- [RPC endpoints](https://docs.arbitrum.io/build-decentralized-apps/reference/node-providers)
- [BoLD documentation](https://docs.arbitrum.io/how-arbitrum-works/bold/gentle-introduction)
- [Gas and fees deep dive](https://docs.arbitrum.io/how-arbitrum-works/deep-dives/gas-and-fees)
- [Timeboost introduction](https://docs.arbitrum.io/how-arbitrum-works/timeboost/gentle-introduction)
- [AnyTrust protocol](https://docs.arbitrum.io/how-arbitrum-works/deep-dives/anytrust-protocol)
- [High-availability sequencer setup](https://docs.arbitrum.io/launch-arbitrum-chain/arbitrum-node-runners/high-availability-sequencer-docs)
