# Deployment

## Docker Compose (Development)

The simplest deployment uses Docker Compose:

```yaml
# docker-compose.yml
services:
  hive:
    image: ghcr.io/copyleftdev/gashammer-hive:latest
    ports:
      - "8080:8080"   # REST API
      - "9090:9090"   # gRPC
      - "9091:9091"   # Prometheus metrics
    volumes:
      - ./hive.toml:/etc/gashammer/hive.toml
      - gashammer-data:/var/lib/gashammer

  edge:
    image: ghcr.io/copyleftdev/gashammer-edge:latest
    environment:
      GASHAMMER_HIVE_ADDRESS: hive:9090
      GASHAMMER_NITRO_SEQUENCER_RPC: http://sequencer:8547
    volumes:
      - ./edge.toml:/etc/gashammer/edge.toml
    deploy:
      replicas: 3

volumes:
  gashammer-data:
```

## Kubernetes (Helm)

For production deployments, use the Helm chart:

```bash
helm repo add gashammer https://copyleftdev.github.io/gashammer/charts
helm install gashammer gashammer/gashammer \
  --set hive.config.nitro.sequencerRpc=http://sequencer:8547 \
  --set edge.replicas=5
```

The Helm chart deploys:
- Hive as a **Deployment** with a **Service** (REST + gRPC)
- Edges as a **Deployment** with configurable replicas
- **ConfigMaps** for hive.toml and edge.toml
- **ServiceMonitor** for Prometheus Operator integration

## Binary Deployment

For bare-metal or VM deployments:

1. Download the release binary for your platform from [GitHub Releases](https://github.com/copyleftdev/gashammer/releases)
2. Verify the signature:
   ```bash
   minisign -Vm gashammer-hive-linux-amd64 -p gashammer.pub
   ```
3. Verify the checksum:
   ```bash
   sha256sum -c SHA256SUMS
   ```
4. Run the binary:
   ```bash
   ./gashammer-hive --config /etc/gashammer/hive.toml
   ```

## Multi-Region Deployment

For multi-region load testing:

1. Deploy one hive in a central location
2. Deploy edges in each target region
3. Configure each edge to connect to the central hive via gRPC
4. All edges connect to the same Nitro deployment (or region-specific endpoints)

```text
Region A                    Central                   Region B
┌─────────┐               ┌─────────┐               ┌─────────┐
│ Edge A  │──── gRPC ────▶│  Hive   │◀──── gRPC ────│ Edge B  │
└────┬────┘               └─────────┘               └────┬────┘
     │                                                    │
     └──────── JSON-RPC ──▶ Nitro ◀── JSON-RPC ──────────┘
```

## Resource Planning

| Edges | Hive CPU | Hive RAM | Hive Disk (per hour) |
|-------|----------|----------|---------------------|
| 1-3 | 2 cores | 4 GB | ~100 MB |
| 4-10 | 4 cores | 8 GB | ~500 MB |
| 11-50 | 8 cores | 16 GB | ~2 GB |
| 50+ | 16 cores | 32 GB | ~10 GB |

Disk usage depends heavily on event volume and Parquet rotation settings.
