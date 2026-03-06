# Configuration

GasHammer uses TOML configuration files for the hive and edge processes. Scenario definitions use YAML (see [Scenario Authoring](../scenarios/index.md)).

## Hive Configuration

The hive reads its configuration from `hive.toml`:

```toml
[server]
bind_address = "0.0.0.0"
rest_port = 8080
grpc_port = 9090

[storage]
data_dir = "/var/lib/gashammer"
parquet_rotation_size_mb = 256
parquet_rotation_interval_secs = 3600

[telemetry]
prometheus_port = 9091

[logging]
level = "info"
format = "json"
```

### Server Section

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `bind_address` | string | `"0.0.0.0"` | Network interface to bind |
| `rest_port` | u16 | `8080` | REST API port |
| `grpc_port` | u16 | `9090` | gRPC port for edge communication |

### Storage Section

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `data_dir` | path | `"/var/lib/gashammer"` | Directory for telemetry data and reports |
| `parquet_rotation_size_mb` | u64 | `256` | Rotate Parquet files at this size |
| `parquet_rotation_interval_secs` | u64 | `3600` | Rotate Parquet files at this interval |

## Edge Configuration

Each edge reads its configuration from `edge.toml`:

```toml
[edge]
name = "edge-us-east-1"

[hive]
address = "hive.internal:9090"

[nitro]
sequencer_rpc = "http://sequencer:8547"
gateway_rpc = "http://gateway:8547"
feed_relay = "ws://feed-relay:9642"

[nitro.rpc]
timeout_ms = 5000
max_retries = 3
connection_pool_size = 16

[accounts]
mnemonic = "test test test test test test test test test test test junk"
count = 100
derivation_base = "m/44'/60'/0'/0'/0x4748"

[telemetry]
buffer_size = 65536
batch_size = 1024
flush_interval_ms = 1000
prometheus_port = 9091

[logging]
level = "info"
format = "json"
```

### Nitro Section

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `sequencer_rpc` | URL | required | Nitro sequencer JSON-RPC endpoint |
| `gateway_rpc` | URL | required | Nitro gateway JSON-RPC endpoint |
| `feed_relay` | URL | required | Sequencer feed relay WebSocket URL |
| `timeout_ms` | u64 | `5000` | RPC request timeout |
| `max_retries` | u32 | `3` | Max retries on transient errors |
| `connection_pool_size` | u32 | `16` | HTTP connection pool size |

### Accounts Section

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `mnemonic` | string | required | HD wallet mnemonic for test accounts |
| `count` | u32 | `100` | Number of accounts to derive |
| `derivation_base` | string | `"m/44'/60'/0'/0'/0x4748"` | HD derivation path (includes GasHammer DNA marker `0x4748`) |

## Environment Variables

Configuration values can be overridden with environment variables using the `GASHAMMER_` prefix:

```bash
GASHAMMER_NITRO_SEQUENCER_RPC=http://localhost:8547
GASHAMMER_HIVE_ADDRESS=hive.internal:9090
GASHAMMER_LOG_LEVEL=debug
```
