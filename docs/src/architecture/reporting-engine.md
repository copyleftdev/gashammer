# Reporting Engine

The reporting engine transforms raw telemetry into actionable results: latency analysis, capacity envelopes, regression detection, and release gate verdicts.

Ref: RFC-0009.

## Run Report

After a run completes, the engine reads Parquet-stored events and produces a `RunReport` containing:

- **Latency percentiles** --- p50, p90, p95, p99, p99.9 for submission, acceptance, and inclusion latencies.
- **Throughput time series** --- gas/sec and tx/sec sampled at 1-second intervals.
- **Error breakdown** --- failure counts by error type.
- **Oracle verdicts** --- pass/fail per invariant with evidence.
- **Capacity envelope** --- if applicable (ramp scenarios).
- **Regression analysis** --- if a baseline is provided.

Reports are available in JSON and Markdown formats via `GET /runs/{id}/report`.

## Capacity Envelope

The capacity envelope defines the **safe operating range** of the system under test. It is computed from ramp-to-saturation scenarios where gas rate increases progressively.

```rust
struct CapacityEnvelope {
    safe_sustained_gas_sec: u64,
    degradation_onset_gas_sec: u64,
    instability_threshold_gas_sec: u64,
    first_bottleneck: BottleneckClassification,
    bottleneck_confidence: Confidence,
    recovery_half_life_sec: f64,
    validator_drift_threshold_gas_sec: Option<u64>,
}
```

### Envelope Boundaries

| Boundary | Meaning |
|----------|---------|
| **Safe sustained** | Gas rate the system handles indefinitely with acceptable latency |
| **Degradation onset** | Rate where p99 latency begins climbing above baseline |
| **Instability threshold** | Rate where errors appear or latency becomes unbounded |

### Bottleneck Classification

The engine identifies the first bottleneck from infrastructure metrics:

| Classification | Indicator |
|----------------|-----------|
| `SequencerCpu` | CPU > 80% at degradation onset |
| `SequencerMemory` | Memory pressure detected |
| `SequencerNetwork` | Network saturation |
| `BatchPosterLag` | Batch posting falls behind |
| `ValidatorReplayLag` | Validator cannot keep up with state |
| `RpcSaturation` | RPC gateway saturated |
| `DiskIo` | Disk IOPS limit reached |
| `GasPriceEscalation` | Base fee escalating under load |
| `Unknown` | No clear indicator |

Confidence is rated `High`, `Medium`, or `Low` based on the quality and consistency of the signal.

### Data Points

The envelope is computed from two input series:

- `RampDataPoint` --- gas rate, measured metric value, and time offset. One per observation during the ramp.
- `InfraMetricsPoint` --- CPU %, memory %, disk IOPS, network bytes/sec, base fee, batch post age, validator lag. Sampled at the same interval.

## Regression Detection

When a baseline run ID is provided, the engine compares the current run against the baseline and flags regressions.

**Comparison dimensions:**

| Metric | Regression Threshold |
|--------|---------------------|
| p50 latency | >10% increase |
| p99 latency | >20% increase |
| Throughput | >10% decrease |
| Error rate | Any increase from 0%, or >50% increase |
| Safe sustained gas rate | >10% decrease |

Results include the baseline value, current value, delta percentage, and whether the regression is flagged.

## Report Formats

| Format | Content-Type | Use Case |
|--------|-------------|----------|
| JSON | `application/json` | CI/CD pipelines, programmatic access |
| Markdown | `text/markdown` | Human review, PR comments, wiki |

Both formats include the complete data --- percentiles, time series, verdicts, envelope, and regression results.
