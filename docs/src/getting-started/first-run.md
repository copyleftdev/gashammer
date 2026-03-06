# First Run

This guide walks through running your first GasHammer load test using the built-in devnet.

## Step 1: Start the Devnet

GasHammer includes a testcontainers-based Nitro devnet. For manual testing, start the containers:

```bash
docker compose -f docker/devnet.yml up -d
```

This starts:
- **L1 Geth** --- Ethereum L1 in dev mode with instant finality
- **Nitro Sequencer** --- Arbitrum Nitro sequencer node
- **Feed Relay** --- Sequencer feed relay for WebSocket subscriptions

Wait for the health checks to pass (typically under 30 seconds).

## Step 2: Start the Hive

```bash
./target/release/gashammer-hive --config hive.toml
```

The hive starts the REST API on port 8080 and the gRPC server on port 9090. Verify it's running:

```bash
curl http://localhost:8080/health
```

## Step 3: Start an Edge

```bash
./target/release/gashammer-edge --config edge.toml
```

The edge registers with the hive, connects to the sequencer, and begins its heartbeat. Verify registration:

```bash
curl http://localhost:8080/edges
```

## Step 4: Submit a Scenario

Use one of the example scenarios:

```bash
curl -X POST http://localhost:8080/scenarios \
  -H "Content-Type: application/yaml" \
  --data-binary @scenarios/simple-transfer-load.yaml
```

## Step 5: Start the Run

```bash
curl -X POST http://localhost:8080/runs \
  -H "Content-Type: application/json" \
  -d '{"scenario": "simple-transfer-load"}'
```

## Step 6: Monitor Progress

Watch the run status:

```bash
# Poll run status
curl http://localhost:8080/runs/{run_id}

# View live metrics (Prometheus)
curl http://localhost:9091/metrics
```

## Step 7: Retrieve the Report

Once the run completes:

```bash
curl http://localhost:8080/runs/{run_id}/report > report.json
```

The report includes:
- Latency percentiles (p50, p90, p95, p99, p99.9)
- Throughput time series (gas/sec and tx/sec)
- Correctness oracle verdicts
- Capacity envelope (if applicable)

## Using the Quick Smoke Test

For a fast validation, use the smoke test scenario:

```bash
curl -X POST http://localhost:8080/scenarios \
  --data-binary @scenarios/quick-smoke-test.yaml

curl -X POST http://localhost:8080/runs \
  -d '{"scenario": "quick-smoke-test"}'
```

This runs a minimal 2-minute test at 1 Mgas/s --- enough to verify the full pipeline works.
