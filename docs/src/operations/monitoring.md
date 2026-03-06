# Monitoring

GasHammer exposes Prometheus metrics from both the hive and edge processes. This page describes how to set up monitoring with Prometheus and Grafana.

## Prometheus Configuration

Add GasHammer targets to your `prometheus.yml`:

```yaml
scrape_configs:
  - job_name: 'gashammer-hive'
    static_configs:
      - targets: ['hive:9091']

  - job_name: 'gashammer-edge'
    static_configs:
      - targets: ['edge-1:9091', 'edge-2:9091', 'edge-3:9091']
```

With Kubernetes and the ServiceMonitor from the Helm chart, Prometheus Operator discovers targets automatically.

## Key Metrics to Watch

### During a Run

| Metric | Alert Threshold | Meaning |
|--------|----------------|---------|
| `gashammer_tx_submitted_total` rate | drops to 0 | Edge stopped generating |
| `gashammer_tx_failed_total` rate | >5% of submitted | High failure rate |
| `gashammer_tx_latency_seconds` p99 | >10s | Severe latency degradation |
| `gashammer_events_dropped_total` | any increase | Telemetry backpressure |
| `gashammer_edges_active` | drops below expected | Edge disconnection |
| `gashammer_oracle_violations_total` | any increase | Correctness issue detected |

### System Health

| Metric | Alert Threshold | Meaning |
|--------|----------------|---------|
| `gashammer_feed_messages_total` rate | drops to 0 | Feed connection lost |
| Edge heartbeat age | >30s | Edge may be stale |
| Hive memory usage | >80% of limit | Risk of OOM |

## Grafana Dashboard

Import the GasHammer Grafana dashboard from `docs/grafana/dashboard.json` or use these example panels:

### Transaction Throughput

```
rate(gashammer_tx_submitted_total[1m])
rate(gashammer_tx_confirmed_total[1m])
```

### Latency Percentiles

```
histogram_quantile(0.50, rate(gashammer_tx_latency_seconds_bucket[1m]))
histogram_quantile(0.90, rate(gashammer_tx_latency_seconds_bucket[1m]))
histogram_quantile(0.99, rate(gashammer_tx_latency_seconds_bucket[1m]))
```

### Gas Rate

```
rate(gashammer_gas_submitted_total[1m])
```

### Error Rate

```
rate(gashammer_tx_failed_total[1m]) / rate(gashammer_tx_submitted_total[1m])
```

## Alerting

Recommended Prometheus alerting rules:

```yaml
groups:
  - name: gashammer
    rules:
      - alert: GasHammerEdgeDown
        expr: gashammer_edges_active < 1
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "No GasHammer edges connected"

      - alert: GasHammerHighErrorRate
        expr: rate(gashammer_tx_failed_total[5m]) / rate(gashammer_tx_submitted_total[5m]) > 0.05
        for: 2m
        labels:
          severity: warning
        annotations:
          summary: "GasHammer error rate exceeds 5%"

      - alert: GasHammerOracleViolation
        expr: increase(gashammer_oracle_violations_total[1m]) > 0
        labels:
          severity: critical
        annotations:
          summary: "Correctness oracle detected a violation"
```

## Structured Logging

Both hive and edge emit structured JSON logs via `tracing`. Configure your log aggregation system to parse these fields:

| Field | Description |
|-------|-------------|
| `timestamp` | ISO 8601 timestamp |
| `level` | Log level (TRACE, DEBUG, INFO, WARN, ERROR) |
| `target` | Rust module path |
| `span` | Current tracing span |
| `run_id` | Associated run (if in a run context) |
| `edge_id` | Edge identifier (edge logs only) |
| `message` | Human-readable message |
