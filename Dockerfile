# Copyright (c) 2025-present Don Johnson
# Licensed under the Business Source License 1.1 (the "License").

# ── Stage 1: Builder ─────────────────────────────────────────────────
FROM rust:1.82-bookworm AS builder

RUN apt-get update && apt-get install -y protobuf-compiler && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY . .

ARG BINARY=gashammer-hive
RUN cargo build --release --bin "${BINARY}"

# ── Stage 2: Runtime ─────────────────────────────────────────────────
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime

ARG BINARY=gashammer-hive
ARG VERSION=0.1.0
ARG BUILD_SHA=unknown

# OCI standard labels
LABEL org.opencontainers.image.title="GasHammer" \
      org.opencontainers.image.description="Adversarial load testing and correctness verification for Arbitrum Nitro rollup infrastructure" \
      org.opencontainers.image.authors="Don Johnson <copyleftdev@github.com>" \
      org.opencontainers.image.url="https://github.com/copyleftdev/gashammer" \
      org.opencontainers.image.source="https://github.com/copyleftdev/gashammer" \
      org.opencontainers.image.licenses="BSL-1.1" \
      org.opencontainers.image.vendor="Don Johnson" \
      org.opencontainers.image.version="${VERSION}"

# GasHammer DNA labels
LABEL com.gashammer.build.sha="${BUILD_SHA}" \
      com.gashammer.build.version="${VERSION}" \
      com.gashammer.binary="${BINARY}"

COPY --from=builder /build/target/release/${BINARY} /usr/local/bin/app

ENTRYPOINT ["/usr/local/bin/app"]
