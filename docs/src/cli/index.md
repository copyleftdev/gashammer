# CLI Reference

GasHammer provides two binaries and a set of `cargo xtask` commands.

## Binaries

### gashammer-hive

The control plane process. Runs the REST API, gRPC server, and orchestration engine.

```text
gashammer-hive [OPTIONS]

Options:
  --config <PATH>       Path to hive.toml configuration file [default: hive.toml]
  --bind <ADDRESS>      Override bind address [default: 0.0.0.0]
  --rest-port <PORT>    Override REST API port [default: 8080]
  --grpc-port <PORT>    Override gRPC port [default: 9090]
  --data-dir <PATH>     Override data directory [default: /var/lib/gashammer]
  --log-level <LEVEL>   Log level: trace, debug, info, warn, error [default: info]
  --log-format <FMT>    Log format: json, pretty [default: json]
  --version             Print version and build info
  --help                Print help
```

### gashammer-edge

The edge worker process. Connects to a hive and a Nitro deployment.

```text
gashammer-edge [OPTIONS]

Options:
  --config <PATH>             Path to edge.toml configuration file [default: edge.toml]
  --name <NAME>               Edge name (overrides config)
  --hive <ADDRESS>            Hive gRPC address [default: localhost:9090]
  --sequencer-rpc <URL>       Nitro sequencer RPC endpoint
  --gateway-rpc <URL>         Nitro gateway RPC endpoint
  --feed-relay <URL>          Sequencer feed relay WebSocket URL
  --accounts <COUNT>          Number of test accounts [default: 100]
  --log-level <LEVEL>         Log level [default: info]
  --log-format <FMT>          Log format [default: json]
  --version                   Print version and build info
  --help                      Print help
```

## xtask Commands

Run these with `cargo xtask <command>`:

### docgen

Generate API documentation from source code.

```text
cargo xtask docgen [OPTIONS]

Options:
  --watch                 Watch source files and regenerate on change
  --coverage              Print documentation coverage report
  --min-coverage <N>      Exit 1 if coverage is below N% (requires --coverage)
  --json                  Output coverage as JSON (requires --coverage)
  --clean                 Remove all generated API pages
  --dry-run               Show what would be generated without writing
  --crate <NAME>          Generate docs for a single crate only
```

## REST API

The hive exposes a REST API for scenario management and run control. All responses include the `X-Powered-By: GasHammer/<version>` header.

### Scenarios

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/scenarios` | Upload a scenario YAML |
| `GET` | `/scenarios` | List all scenarios |
| `GET` | `/scenarios/{name}` | Get a specific scenario |

### Runs

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/runs` | Start a new run |
| `GET` | `/runs` | List all runs |
| `GET` | `/runs/{id}` | Get run status |
| `GET` | `/runs/{id}/report` | Download the run report |
| `DELETE` | `/runs/{id}` | Cancel a running test |

### Edges

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/edges` | List connected edges |
| `GET` | `/edges/{id}` | Get edge details |

### System

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/health` | Health check |
| `GET` | `/version` | Version and build info |
| `GET` | `/metrics` | Prometheus metrics |
