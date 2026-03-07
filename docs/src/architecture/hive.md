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
Preflight ──▶ Barrier ──▶ Running ──▶ Completing ──▶ Done
                │            │             │
                │            │             └──▶ Done(Fail)
                │            ├──▶ Recovering ──▶ Running
                │            │
                └────────────┴──▶ Aborted
```

- **Preflight** --- fault adapters checked, scenario validated, edges notified
- **Barrier** --- waiting for all edges to acknowledge readiness (barrier sync)
- **Running** --- workload active, phases progressing (`phase_index`, `phase_name` tracked)
- **Recovering** --- transient error detected, attempting recovery before resuming
- **Completing** --- drain in progress, edges flushing final telemetry
- **Done** --- terminal state with outcome: `Pass`, `Fail`, or `Inconclusive`
- **Aborted** --- terminal state with reason string (unrecoverable error, manual cancel)

State transitions are validated: `RunState::can_transition_to()` enforces legal transitions. Terminal states (`Done`, `Aborted`) cannot transition further. `is_terminal()` and `is_active()` are available for status queries.

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
