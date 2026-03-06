# RFC-0009: Reporting & Regression Engine

## Status

Draft

## Author

Don Johnson

## Depends On

- RFC-0006 (Telemetry & Metrics Pipeline)
- RFC-0007 (Correctness Oracle)

## Summary

This RFC defines the reporting and regression engine — the subsystem that transforms raw telemetry and oracle verdicts into **decision-grade outputs** (PRD §6.6). Reports must answer specific engineering questions: Is this build safe to ship? What is the capacity envelope? What broke first? How does this compare to last release?

---

## 1. Report Types

GasHammer produces four report types (PRD §14):

| Report | When | Primary Audience |
| --- | --- | --- |
| **Standard Run Report** | After every run | All users |
| **Capacity Envelope Report** | After ramp-to-failure scenarios | Performance engineers, SREs |
| **Release Gate Report** | When comparing against a baseline | Release managers, CI pipelines |
| **Incident Replay Report** | After replay scenarios | Incident responders |

---

## 2. Standard Run Report

### 2.1 Contents

```text
1. Run Metadata
   - scenario name, run_id, seed, timestamp, duration
   - target environment (chain ID, endpoints, Nitro version if known)
   - GasHammer version (hive, edge, scenario compiler)

2. Edge Topology
   - edge count by region
   - edge health summary
   - edges lost during run (if any)

3. Phase Timeline
   - per-phase: start/end time, duration, gas rate achieved
   - fault injection events overlaid on timeline

4. Throughput Summary
   - gas/sec submitted (aggregate + per-region)
   - gas/sec accepted (aggregate + per-region)
   - gas/sec included (aggregate + per-region)
   - tx/sec by workload class
   - revert rate by workload class

5. Latency Summary
   - accept latency: p50, p95, p99, max (aggregate + per-region)
   - include latency: p50, p95, p99, max (aggregate + per-region)
   - batch latency: p50, p95, p99 (from L1 observation)

6. Infrastructure Summary (if Prometheus available)
   - target CPU, memory, disk, network peaks
   - correlation: latency spikes vs. resource spikes

7. Correctness Summary
   - per-check pass/fail
   - violation count
   - evidence links

8. Regression Status (if baseline provided)
   - key deltas vs. baseline

9. Artifacts
   - links to raw Parquet data, scenario YAML, reproducibility bundle
```

### 2.2 Output Formats

| Format | Use Case |
| --- | --- |
| **JSON** | Machine-readable; CI pipeline consumption; API response |
| **HTML** (static) | Human-readable; shareable via browser |
| **Markdown** | Embeddable in PRs, Slack, wikis |
| **PDF** | Formal distribution (future) |

MVP: JSON + Markdown. HTML static site as stretch goal.

---

## 3. Capacity Envelope Report

### 3.1 Purpose

Answers: "What is the safe operating range of this system?"

### 3.2 Derivation

Requires a **ramp-to-failure** scenario (RFC-0005 §4.2) where gas rate increases linearly over time. The report engine analyzes the telemetry to find inflection points:

```rust
pub struct CapacityEnvelope {
    pub safe_sustained_gas_sec: u64,
    pub degradation_onset_gas_sec: u64,
    pub instability_threshold_gas_sec: u64,
    pub first_bottleneck: BottleneckClassification,
    pub recovery_half_life_sec: f64,
    pub validator_drift_threshold_gas_sec: Option<u64>,
}

pub enum BottleneckClassification {
    SequencerCpu,
    SequencerMemory,
    SequencerNetwork,
    BatchPosterLag,
    ValidatorReplayLag,
    RpcSaturation,
    DiskIo,
    GasPriceEscalation,
    Unknown { evidence: String },
}
```

### 3.3 Inflection Point Detection

The engine scans the time series for:

- **Degradation onset:** The gas rate at which p99 accept latency exceeds 2× baseline (first 10% of ramp)
- **Instability threshold:** The gas rate at which any of: revert rate >5%, feed stalls >1s, or correctness violations occur
- **Validator drift:** The gas rate at which validator lag exceeds the configured threshold (default: 3 blocks)
- **Recovery half-life:** After the ramp ends, how long until p99 latency returns to 50% of peak

### 3.4 Bottleneck Classification

The engine correlates latency inflection with infrastructure metrics:

1. If CPU spikes at the same time as latency degradation → CPU bottleneck
2. If memory pressure rises → memory bottleneck
3. If disk IOPS saturates → disk I/O bottleneck
4. If batch posting age increases → batch poster lag
5. If base fee escalates rapidly → gas price escalation (chain protecting itself)
6. If no infra metric correlates → unknown (requires deeper investigation)

Classification is heuristic, not definitive. The report labels confidence:

```json
{
  "first_bottleneck": "sequencer_cpu",
  "confidence": "high",
  "evidence": "CPU utilization reached 95% at t=12m, coinciding with p99 latency jump from 45ms to 320ms"
}
```

---

## 4. Release Gate Report

### 4.1 Purpose

Answers: "Did this build regress relative to the baseline?"

### 4.2 Baseline Selection

A baseline is a previous run identified by `run_id` or a named tag (e.g., `release/v3.9.4`):

```yaml
regression:
  baseline: "run_id:abc123"  # or "tag:release/v3.9.4"
  thresholds:
    p99_accept_latency_ms:
      max_regression_pct: 10    # fail if p99 regresses by >10%
    max_sustainable_gas_sec:
      min_value: 50000000       # absolute minimum
      max_regression_pct: 5
    correctness_violations:
      max_value: 0
    revert_rate:
      max_value: 0.05
```

### 4.3 Diffing Algorithm

For each metric in the threshold set:

```rust
pub struct RegressionDelta {
    pub metric_name: String,
    pub baseline_value: f64,
    pub current_value: f64,
    pub delta_absolute: f64,
    pub delta_pct: f64,
    pub threshold_pct: f64,
    pub verdict: RegressionVerdict,
}

pub enum RegressionVerdict {
    Pass,               // within threshold
    Improvement,        // better than baseline beyond threshold
    Regression,         // worse than baseline beyond threshold
    Inconclusive,       // insufficient data or high variance
}
```

### 4.4 Gate Output

```json
{
  "gate_verdict": "FAIL",
  "regressions": [
    {
      "metric": "p99_accept_latency_ms",
      "baseline": 45.2,
      "current": 62.8,
      "delta_pct": 38.9,
      "threshold_pct": 10.0,
      "verdict": "REGRESSION"
    }
  ],
  "improvements": [],
  "pass_count": 3,
  "fail_count": 1,
  "links": {
    "full_report": "/api/v1/runs/{run_id}/report",
    "baseline_report": "/api/v1/runs/{baseline_id}/report",
    "artifacts": "/api/v1/runs/{run_id}/artifacts"
  }
}
```

### 4.5 CI Integration

The hive REST API returns the gate verdict with an appropriate HTTP status:

- `200 OK` + `gate_verdict: PASS` → CI step passes
- `200 OK` + `gate_verdict: FAIL` → CI step fails (non-error failure)
- `503 Service Unavailable` → run not yet complete

CI pipelines can poll `GET /api/v1/runs/{id}/gate` until the run completes.

---

## 5. Incident Replay Report

### 5.1 Purpose

Answers: "Does this scenario reproduce the observed production symptoms?"

### 5.2 Structure

```text
1. Production Incident Summary
   - incident ID, timestamp, duration
   - observed symptoms (from scenario metadata)

2. Replay Configuration
   - scenario used, workload shape, scaling factor
   - target environment differences from production

3. Symptom Correlation
   - for each observed production symptom:
     - was it reproduced? (yes/partial/no)
     - GasHammer evidence (metric, timestamp, value)
     - confidence level

4. Candidate Root Causes
   - bottleneck classification from replay
   - timeline of degradation events
   - infra metric correlations

5. Confidence Notes
   - limitations of the replay (§differences from production)
   - variance in repeated replays
   - known gaps in workload fidelity
```

### 5.3 Symptom Matching

Incident replay scenarios include expected symptoms:

```yaml
incident_replay:
  incident_id: "INC-2025-0301"
  expected_symptoms:
    - name: "p99 latency spike >500ms"
      metric: accept_latency_p99_ms
      condition: "gt"
      value: 500
    - name: "validator lag >5 blocks"
      metric: validator_lag_blocks
      condition: "gt"
      value: 5
    - name: "batch posting delay >5 min"
      metric: batch_latency_p99_ms
      condition: "gt"
      value: 300000
```

The report engine checks each expected symptom against the actual telemetry and reports reproduction confidence.

---

## 6. Report Generation Pipeline

```text
Run Completes
    │
    ▼
┌──────────────┐
│ Data Fetch   │  Read aggregates from PostgreSQL + raw from S3
└──────┬───────┘
       ▼
┌──────────────┐
│ Compute      │  Percentiles, rates, deltas, inflection points
└──────┬───────┘
       ▼
┌──────────────┐
│ Classify     │  Bottleneck classification, regression detection
└──────┬───────┘
       ▼
┌──────────────┐
│ Merge Oracle │  Integrate correctness verdicts
└──────┬───────┘
       ▼
┌──────────────┐
│ Format       │  Generate JSON + Markdown + HTML
└──────┬───────┘
       ▼
┌──────────────┐
│ Store        │  Write to S3 as artifacts; update run metadata
└──────────────┘
```

### 6.1 Data Access

The report engine reads:

- **PostgreSQL:** Run metadata, phase timeline, aggregated metrics, oracle verdicts
- **S3 (Parquet):** Raw events for detailed analysis (percentile computation, time series plots)
- **S3 (Prometheus snapshots):** Target node metrics

For standard reports, aggregated data from PostgreSQL is sufficient. Capacity envelope and incident replay reports may require raw Parquet data for inflection point detection.

### 6.2 Templating

Markdown and HTML reports use a template engine (`tera` crate — Jinja2-compatible for Rust):

```text
deploy/templates/
├── run_report.md.tera
├── capacity_envelope.md.tera
├── release_gate.md.tera
├── incident_replay.md.tera
└── partials/
    ├── latency_table.md.tera
    ├── throughput_chart.md.tera
    └── correctness_summary.md.tera
```

---

## 7. Visualization (Stretch Goal)

### 7.1 Static HTML Report

For the HTML output format, include embedded charts:

- **Throughput timeline:** gas/sec over time with phase boundaries and fault events
- **Latency heatmap:** per-region latency percentiles over time
- **Bottleneck timeline:** infrastructure metrics overlaid with latency
- **Capacity envelope curve:** gas rate vs. p99 latency with annotated thresholds

Implementation: Generate chart data as JSON; embed with a lightweight JS chart library (e.g., Chart.js or Plotly.js) in a self-contained HTML file.

### 7.2 Dashboard (Post-MVP)

A live dashboard during runs is post-MVP. For MVP, the Prometheus metrics endpoint (RFC-0006 §4.5) can be consumed by Grafana for live monitoring.

---

## 8. Artifact Bundle

Every run produces an immutable artifact bundle stored in S3:

```text
artifacts/{run_id}/
├── scenario.yaml            # original scenario definition
├── compiled_scenario.json   # compiled per-edge assignments
├── report.json              # structured report
├── report.md                # markdown report
├── oracle_verdicts.json     # full correctness results
├── events/                  # raw Parquet event files
│   ├── edge-{id}/
│   │   ├── phase-0/
│   │   │   └── chunk-001.parquet
│   │   └── ...
│   └── ...
├── prometheus/              # scraped target metrics
│   └── snapshots.parquet
└── manifest.json            # bundle contents + hashes
```

`manifest.json` includes content hashes for every file, enabling verification of bundle integrity.

---

## 9. Risks

### Risk: Bottleneck classification is wrong

Heuristic classification may misidentify the root cause.

**Mitigation:** Label all classifications with confidence levels. Include raw evidence so engineers can verify. Never present classification as definitive — always "likely first bottleneck."

### Risk: Regression detection is noisy

Run-to-run variance may trigger false regressions.

**Mitigation:** Support multiple baseline runs for variance estimation. Use statistical tests (Welch's t-test or Mann-Whitney U) when multiple baselines are available. Label regressions as "statistically significant" or "within variance."

### Risk: Report generation is slow for large runs

A 30-minute run with 100 edges produces billions of events.

**Mitigation:** Report engine works primarily from pre-aggregated data (PostgreSQL). Only accesses raw Parquet for specific analyses (capacity envelope inflection detection). Parquet columnar reads are efficient for scanning specific metrics.

### Risk: Artifact storage costs grow unbounded

Each run produces gigabytes of Parquet data.

**Mitigation:** Configurable retention policy (default 90 days). S3 lifecycle rules for automatic expiration. Option to retain only aggregated data + report, discarding raw events after N days.

---

## 10. Open Questions

1. Should the report engine support custom report templates defined by users?
2. What statistical test is appropriate for regression detection with small sample sizes (2-3 baseline runs)?
3. Should capacity envelope reports auto-generate a recommended gas target for production?
4. Should reports include cost estimates (L1 gas cost for the test run itself)?
5. What is the minimum viable report for the interview demo — full HTML or Markdown-only?

---

## 11. References

- PRD §6.6 (Outputs must be decision-grade)
- PRD §14 (Reporting and Outputs — all four report types)
- PRD §10.7 (Reporting requirements)
- RFC-0006 §4 (Ingestion Pipeline — data sources)
- RFC-0006 §8 (Storage Model)
- RFC-0007 §6 (Verdict Aggregation)
