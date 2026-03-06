# Error Codes

Every GasHammer error carries a structured code in the format `GH-Exxx`. The prefix `GH-E` is part of GasHammer's structural DNA (RFC-0011 §8.3). Codes are unique across the entire codebase and grouped by crate.

## Code Ranges

| Range | Crate | Category |
|-------|-------|----------|
| E001--E099 | gashammer-common | Shared infrastructure |
| E100--E199 | gashammer-nitro | Nitro protocol adapters |
| E200--E299 | gashammer-edge | Edge runtime |
| E300--E399 | gashammer-hive | Hive control plane |
| E400--E499 | gashammer-workload | Workload engine |
| E500--E599 | gashammer-telemetry | Telemetry pipeline |
| E600--E699 | gashammer-oracle | Correctness oracle |
| E700--E799 | gashammer-fault | Fault injection |
| E800--E899 | gashammer-report | Reporting engine |
| E900--E949 | gashammer-scenario | SDL parser/compiler |
| E950--E979 | gashammer-docgen | Documentation engine |
| E980--E999 | gashammer-testenv | Test environment |

## Complete Registry

### Common (E001--E099)

| Code | Name | Description |
|------|------|-------------|
| `GH-E001` | `ConfigError` | Configuration loading or validation failed |
| `GH-E002` | `SerializationError` | Serialization or deserialization failed |
| `GH-E003` | `InvariantViolation` | An internal invariant was violated |

### Nitro (E100--E199)

| Code | Name | Description |
|------|------|-------------|
| `GH-E100` | `RpcError` | JSON-RPC call failed |
| `GH-E101` | `FeedError` | Sequencer feed connection or parsing failed |
| `GH-E102` | `L1ContractError` | L1 contract read failed |

### Edge (E200--E299)

| Code | Name | Description |
|------|------|-------------|
| `GH-E200` | `EdgeRegistrationFailed` | Edge failed to register with the hive |
| `GH-E201` | `TxPipelineError` | Transaction pipeline error |
| `GH-E202` | `FeedCorrelationError` | Feed correlation error |

### Hive (E300--E399)

| Code | Name | Description |
|------|------|-------------|
| `GH-E300` | `OrchestrationError` | Run orchestration failed |
| `GH-E301` | `EdgeRegistryError` | Edge registry operation failed |
| `GH-E302` | `ApiError` | REST API error |

### Workload (E400--E499)

| Code | Name | Description |
|------|------|-------------|
| `GH-E400` | `TemplateError` | Transaction template construction failed |
| `GH-E401` | `RateControlError` | Rate control error |
| `GH-E402` | `AccountPoolError` | Account pool exhausted or nonce error |

### Telemetry (E500--E599)

| Code | Name | Description |
|------|------|-------------|
| `GH-E500` | `EventShippingError` | Event shipping failed |
| `GH-E501` | `StorageWriteError` | Parquet storage write failed |
| `GH-E502` | `MetricError` | Metric registration or recording failed |

### Oracle (E600--E699)

| Code | Name | Description |
|------|------|-------------|
| `GH-E600` | `CheckExecutionError` | Invariant check execution failed |
| `GH-E601` | `EvidenceError` | Evidence collection failed |
| `GH-E602` | `VerdictAggregationError` | Verdict aggregation error |

### Fault (E700--E799)

| Code | Name | Description |
|------|------|-------------|
| `GH-E700` | `FaultInjectionError` | Fault injection failed |
| `GH-E701` | `FaultReversionError` | Fault reversion (clear) failed |
| `GH-E702` | `FaultSchedulingError` | Fault scheduling error |

### Report (E800--E899)

| Code | Name | Description |
|------|------|-------------|
| `GH-E800` | `ReportGenerationError` | Report generation failed |
| `GH-E801` | `RegressionAnalysisError` | Regression analysis failed |
| `GH-E802` | `ReportTemplateError` | Template rendering failed |

### Scenario (E900--E949)

| Code | Name | Description |
|------|------|-------------|
| `GH-E900` | `ScenarioParseError` | Scenario YAML parsing failed |
| `GH-E901` | `ScenarioValidationError` | Scenario validation failed |
| `GH-E902` | `ScenarioCompilationError` | Scenario compilation failed |

### Docgen (E950--E979)

| Code | Name | Description |
|------|------|-------------|
| `GH-E950` | `SourceParseError` | Source file parsing failed |
| `GH-E951` | `DocgenTemplateError` | Template rendering failed |
| `GH-E952` | `DocgenOutputError` | mdBook output write failed |

### Testenv (E980--E999)

| Code | Name | Description |
|------|------|-------------|
| `GH-E980` | `ContainerError` | Container startup failed |
| `GH-E981` | `HealthCheckError` | Devnet health check failed |
| `GH-E982` | `ContractDeployError` | Contract deployment failed |

## Using Error Codes

All error types use `thiserror` and embed the error code in the `Display` impl:

```rust
#[derive(Debug, thiserror::Error)]
pub enum NitroError {
    #[error("[{code}] {msg}", code = ErrorCode::RpcError.code())]
    Transport(String),
    // ...
}
```

In log output and API responses, errors appear as:

```
[GH-E100] RPC call failed: connection refused
```

Search logs for a specific code with `grep "GH-E100"` to find all instances of a particular error class.
