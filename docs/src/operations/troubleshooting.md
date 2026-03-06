# Troubleshooting

## Edge Cannot Connect to Hive

**Symptom:** Edge logs show `Failed to register with hive` or `gRPC connection refused`.

**Checks:**
1. Verify the hive is running and the gRPC port (default 9090) is accessible
2. Check network connectivity: `nc -zv hive-host 9090`
3. If using mTLS, verify certificates are valid and not expired
4. Check firewall rules between edge and hive

## Edge Cannot Connect to Sequencer

**Symptom:** Edge logs show `RPC connection failed` or `timeout waiting for eth_chainId`.

**Checks:**
1. Verify the sequencer RPC URL is correct and accessible
2. Test with curl: `curl -X POST -H "Content-Type: application/json" -d '{"jsonrpc":"2.0","method":"eth_chainId","params":[],"id":1}' http://sequencer:8547`
3. Check that the chain ID matches the scenario's `target.chain_id`
4. Verify connection pool settings are not exhausting the sequencer's connection limit

## High Transaction Failure Rate

**Symptom:** `gashammer_tx_failed_total` is increasing rapidly.

**Common causes:**
- **Nonce too low** --- another process is using the same accounts. Ensure account partitioning has no overlap.
- **Insufficient funds** --- test accounts need ETH for gas. Check pre-funding.
- **Gas price too low** --- if the network has a base fee, the tx may be underpriced.
- **Sequencer overloaded** --- reduce the gas rate and check sequencer health.

## Feed Correlation Misses

**Symptom:** `gashammer_correlation_misses` is high.

**Common causes:**
- Feed relay WebSocket disconnected (check `gashammer_feed_messages_total` rate)
- Transaction was included in a block but the feed message was missed during reconnection
- Transaction was dropped by the sequencer (check if receipt exists)

## Telemetry Events Dropped

**Symptom:** `gashammer_events_dropped_total` is increasing.

**Resolution:**
- Increase `telemetry.buffer_size` in edge.toml (default 65536)
- Increase `telemetry.batch_size` for larger batches (default 1024)
- Check network bandwidth between edge and hive
- Reduce the workload gas rate if the edge cannot keep up

## Oracle Violation Detected

**Symptom:** `gashammer_oracle_violations_total` increased, report shows failed verdicts.

**This may be expected** --- the oracle detects real issues in the system under test. Review the violation evidence in the report:

- **Balance conservation failure** --- account balances changed by an unexpected amount. Check for unexpected contract interactions.
- **Nonce monotonicity failure** --- a transaction was included out of nonce order. This may indicate a sequencer issue.
- **Gas accounting failure** --- `gas_used` exceeded `gas_limit`. This should never happen and indicates a VM bug.

## Run Stuck in "Starting" State

**Symptom:** Run status remains "Starting" indefinitely.

**Checks:**
1. At least one edge must be registered. Check `GET /edges`.
2. All registered edges must acknowledge the run. Check edge logs for errors.
3. If an edge crashed during startup, the hive waits for it. Restart the edge or deregister it.

## Report Generation Fails

**Symptom:** `GET /runs/{id}/report` returns an error.

**Checks:**
1. Verify the run completed (status "Completed" or "Failed")
2. Check that Parquet files exist in `{data_dir}/runs/{run_id}/`
3. Check hive logs for Parquet read errors
4. Ensure sufficient disk space for report generation

## Build Errors

**Symptom:** `cargo build --workspace` fails.

**Common fixes:**
- Run `cargo update` to update the lock file
- Ensure Rust version is 1.75.0 or later: `rustc --version`
- For protobuf compilation errors: ensure `protoc` is installed
- For Docker-related test failures: ensure Docker daemon is running

## Getting Help

If you cannot resolve an issue:

1. Check the [GitHub Issues](https://github.com/copyleftdev/gashammer/issues) for similar problems
2. Open a new issue with:
   - GasHammer version (`--version` output)
   - Relevant configuration (redact secrets)
   - Log output at `debug` level
   - Steps to reproduce
