FROM rust:1.87-slim AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

# Issue #401: Multi-stage build with cargo-chef for efficient layer caching
# Stage 1: Generate recipe from Cargo.lock (fast, cached unless dependencies change)
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Stage 2: Build dependencies only (cached unless Cargo.lock changes)
FROM chef AS builder
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# Stage 3: Build application (fast rebuild on source changes, dependencies cached)
COPY . .
RUN cargo build --release

# Final stage: Runtime image (minimal size)
# debian:bookworm-slim — digest pinned 2025-07-14. Update via Dependabot or manually with:
# docker inspect --format='{{index .RepoDigests 0}}' debian:bookworm-slim
FROM debian:bookworm-slim@sha256:8af0e5095f9964007f5ebd11191dfe52dcb51bf3afa2c07f055fc5451b78ba0e
RUN apt-get update && apt-get install -y ca-certificates libssl3 curl && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 soroban && useradd --uid 10001 --gid soroban --no-create-home --shell /usr/sbin/nologin soroban

WORKDIR /app
COPY --from=builder --chown=soroban:soroban /app/target/release/soroban-pulse .
COPY --from=builder --chown=soroban:soroban /app/migrations ./migrations

USER soroban:soroban
EXPOSE 3000
HEALTHCHECK --interval=10s --timeout=5s --start-period=30s --retries=5 \
  CMD curl -f http://localhost:3000/healthz/ready || exit 1
CMD ["./soroban-pulse"]
