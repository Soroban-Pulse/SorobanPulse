FROM rust:1.95-slim AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release

# debian:bookworm-slim — digest pinned 2025-07-14. Update via Dependabot or manually with:
# docker inspect --format='{{index .RepoDigests 0}}' debian:bookworm-slim
FROM debian:bookworm-slim@sha256:8af0e5095f9964007f5ebd11191dfe52dcb51bf3afa2c07f055fc5451b78ba0e
RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/soroban-pulse .
COPY --from=builder /app/migrations ./migrations

EXPOSE 3000
CMD ["./soroban-pulse"]
