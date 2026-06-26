# ── Stage 1: build ─────────────────────────────────────────────────────────
ARG PG_VERSION=18
# Full image tag (not just the version) so Renovate can manage the ARG default
# directly. A partial variable like rust:${RUST_VERSION}-slim-bookworm breaks
# Renovate's auto-replace ("Cannot find replaceString in current file content").
ARG RUST_VERSION=1.96-slim-bookworm

FROM rust:${RUST_VERSION} AS builder

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
COPY lib/Cargo.toml lib/Cargo.lock ./
COPY lib/src ./src

# cargo build --release is sufficient: we are a validator module, not a
# CREATE EXTENSION extension, so no .control file is needed.
# Rust names the cdylib libpg_oauth.so; rename to pg_oauth.so for PostgreSQL.
#
# Stage the CloudNativePG extension-image layout under /out:
#   /lib/pg_oauth.so      -> mounted at /extensions/pg_oauth/lib (dynamic_library_path)
#   /share/extension/     -> mounted at /extensions/pg_oauth/share (empty: no SQL
#                            objects, since this is an OAuth validator module)
RUN cargo build --release --features pg18 \
    && cp target/release/libpg_oauth.so target/release/pg_oauth.so \
    && strip target/release/pg_oauth.so \
    && mkdir -p /out/lib /out/share/extension \
    && cp target/release/pg_oauth.so /out/lib/pg_oauth.so

# ── Stage 2: CNPG extension image (image-volume content only) ───────────────
# A scratch image holding just the files CloudNativePG mounts into the Postgres
# pod via a Kubernetes ImageVolume. Not a runnable image.
FROM scratch AS final

COPY --from=builder /out/ /
