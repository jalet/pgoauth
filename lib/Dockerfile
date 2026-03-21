# ── Stage 1: build ─────────────────────────────────────────────────────────
ARG PG_VERSION=18
ARG RUST_VERSION=1.94

FROM rust:${RUST_VERSION}-slim-bookworm AS builder

# Re-declare ARGs after FROM so they are available within this build stage
ARG PG_VERSION=18

ENV DEBIAN_FRONTEND=noninteractive

# Add PostgreSQL APT repo with signed key (HTTPS, scoped key)
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential pkg-config curl gnupg ca-certificates \
    && curl -fsSL https://www.postgresql.org/media/keys/ACCC4CF8.asc \
         | gpg --dearmor -o /etc/apt/trusted.gpg.d/pgdg.gpg \
    && echo "deb [signed-by=/etc/apt/trusted.gpg.d/pgdg.gpg] \
         https://apt.postgresql.org/pub/repos/apt bookworm-pgdg main" \
         > /etc/apt/sources.list.d/pgdg.list \
    && apt-get update \
    && apt-get install -y --no-install-recommends \
         postgresql-server-dev-${PG_VERSION} clang \
    && rm -rf /var/lib/apt/lists/*

# Install cargo-pgrx pinned to match Cargo.toml dependency.
# pgrx init registers the system pg_config so build.rs can find PG headers.
RUN cargo install cargo-pgrx --version "=0.17.0" --locked \
    && cargo pgrx init --pg${PG_VERSION} /usr/bin/pg_config

# Point pgrx build.rs at the system pg_config explicitly.
ENV PG_CONFIG=/usr/bin/pg_config

WORKDIR /build
COPY lib/src/ .

# cargo build --release is sufficient: we are a validator module, not a
# CREATE EXTENSION extension, so no .control file is needed.
# Rust names the cdylib libpg_oauth.so; rename to pg_oauth.so for PostgreSQL.
RUN cargo build --release --features pg18 \
    && cp target/release/libpg_oauth.so target/release/pg_oauth.so \
    && strip target/release/pg_oauth.so

# ── Stage 2: minimal artifact image ────────────────────────────────────────
FROM ubuntu:24.04 AS final

WORKDIR /artifacts

COPY --from=builder /build/target/release/pg_oauth.so ./

CMD ["bash", "-c", "echo 'Artifacts:' && ls -lh /artifacts/"]
