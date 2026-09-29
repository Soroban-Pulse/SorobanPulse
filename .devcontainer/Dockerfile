# ── SorobanPulse Dev Container Image ─────────────────────────────────────────
#
# Installs everything a new contributor needs:
#   • Rust stable toolchain + clippy + rustfmt + cargo-nextest + sqlx-cli
#   • Node 20 (LTS) + npm  — for frontend/SDK work and openapi-generator
#   • Java 17 (Temurin)    — for openapi-generator-cli
#   • make + just           — task runners
#   • Common CLI tools      — git, curl, psql, jq, wget, etc.
#
# This Dockerfile is NOT used in production. The production image lives at
# the repo root as `Dockerfile`.
# ─────────────────────────────────────────────────────────────────────────────

FROM mcr.microsoft.com/devcontainers/base:ubuntu-22.04

# ── Non-interactive apt ───────────────────────────────────────────────────────
ENV DEBIAN_FRONTEND=noninteractive

# ── System packages ───────────────────────────────────────────────────────────
RUN apt-get update && apt-get install -y --no-install-recommends \
    # Build essentials
    build-essential \
    pkg-config \
    libssl-dev \
    # PostgreSQL client (psql for manual inspection + migration scripts)
    postgresql-client \
    # Utilities
    curl \
    wget \
    git \
    jq \
    unzip \
    ca-certificates \
    gnupg \
    lsb-release \
    # make is usually present; explicit to be safe
    make \
    # Needed by some Rust crates
    libpq-dev \
    && rm -rf /var/lib/apt/lists/*

# ── Java 17 (Eclipse Temurin) ─────────────────────────────────────────────────
# Required for openapi-generator-cli (used by `make generate-sdk`).
RUN wget -qO - https://packages.adoptium.net/artifactory/api/gpg/key/public \
      | gpg --dearmor -o /usr/share/keyrings/adoptium.gpg && \
    echo "deb [signed-by=/usr/share/keyrings/adoptium.gpg] \
      https://packages.adoptium.net/artifactory/deb \
      $(lsb_release -cs) main" \
      > /etc/apt/sources.list.d/adoptium.list && \
    apt-get update && \
    apt-get install -y --no-install-recommends temurin-17-jdk && \
    rm -rf /var/lib/apt/lists/*

ENV JAVA_HOME=/usr/lib/jvm/temurin-17-amd64

# ── Node 20 LTS ───────────────────────────────────────────────────────────────
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - && \
    apt-get install -y --no-install-recommends nodejs && \
    rm -rf /var/lib/apt/lists/*

# ── Rust stable toolchain ─────────────────────────────────────────────────────
# Install as the vscode user so cargo home is in /home/vscode/.cargo
USER vscode

ENV CARGO_HOME=/usr/local/cargo
ENV RUSTUP_HOME=/usr/local/rustup
ENV PATH=$CARGO_HOME/bin:$PATH

RUN sudo mkdir -p $CARGO_HOME $RUSTUP_HOME && \
    sudo chown vscode:vscode $CARGO_HOME $RUSTUP_HOME && \
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --no-modify-path \
            --default-toolchain stable \
            --component clippy rustfmt && \
    rustup component add clippy rustfmt

# ── Cargo tools ───────────────────────────────────────────────────────────────
RUN cargo install --locked \
    # SQLx CLI: run/revert migrations, prepare offline query cache
    sqlx-cli --no-default-features --features postgres \
    # Faster test runner (cargo nextest)
    cargo-nextest \
    # just: command runner (alternative to make for complex recipes)
    just \
    && cargo cache --autoclean 2>/dev/null || true

# ── Shell configuration ───────────────────────────────────────────────────────
# Ensure PATH includes Cargo bin for interactive shells.
RUN echo 'export PATH="$CARGO_HOME/bin:$PATH"' >> /home/vscode/.bashrc && \
    echo 'export PATH="$CARGO_HOME/bin:$PATH"' >> /home/vscode/.profile

USER root

# ── Final metadata ────────────────────────────────────────────────────────────
LABEL org.opencontainers.image.description="SorobanPulse development container"
LABEL org.opencontainers.image.source="https://github.com/Soroban-Pulse/SorobanPulse"
