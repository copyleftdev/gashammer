# Hive Control Plane

The hive is GasHammer's central control plane. It coordinates runs across edges, collects telemetry, and produces reports.

## Responsibilities

- **Edge management** --- register, track heartbeats, detect stale edges
- **Scenario validation** --- parse and compile SDL scenarios
- **Run orchestration** --- start runs, synchronize phases across edges, handle completion and failure
- **Telemetry collection** --- receive event streams from edges, write to Parquet
- **Report generation** --- compute latency percentiles, capacity envelopes, regression analysis
- **REST API** --- human and CI interface for all operations

## Run State Machine

```text
Created ──▶ Starting ──▶ Running ──▶ Stopping ──▶ Completed
               │            │            │
               │            │            └──▶ Failed
               │            └──────────────▶ Failed
               └────────────────────────────▶ Failed
```

- **Created** --- scenario validated, run ID assigned
- **Starting** --- edges notified, waiting for all edges to acknowledge
- **Running** --- workload active, phases progressing with barrier sync
- **Stopping** --- drain in progress, edges flushing final telemetry
- **Completed** --- all phases done, report generated
- **Failed** --- error occurred (edge disconnection, timeout, invariant violation)

## Phase Synchronization

The hive coordinates phase transitions using **barrier sync**: all edges must complete the current phase before any edge starts the next. This ensures consistent behavior across a distributed test.

If an edge misses the barrier within the configured timeout, the hive marks it as stale and proceeds with the remaining edges. The stale edge is flagged in the report.

## Edge Registry

The registry tracks connected edges with:
- Edge ID (UUID)
- Registration time
- Last heartbeat timestamp
- Edge capabilities (reported during registration)
- Current status (idle, running, draining, stale)

A reaper task periodically removes stale edges that have missed heartbeats beyond the configured timeout.

## Telemetry Sink

Edges stream telemetry events to the hive via gRPC. The hive buffers events and writes them to Parquet files, partitioned by run ID and time. Parquet metadata includes GasHammer DNA provenance (version, build SHA, copyright).

## Configuration Reference

See [Hive Configuration](../getting-started/configuration.md#hive-configuration).
