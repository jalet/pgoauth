# -----------------------------------------------------------------------------
# Stage 1 — build pg_oauth.so inside the same environment as CNPG Postgres 18
# -----------------------------------------------------------------------------
FROM ghcr.io/cloudnative-pg/postgresql:18.0 AS builder
ENV DEBIAN_FRONTEND=noninteractive
ENV USE_PGXS=1

USER root

# Install build tools and dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential git pkg-config curl \
    libcurl4-openssl-dev libjwt-dev libjansson-dev flex bison && \
    rm -rf /var/lib/apt/lists/*


USER 26

# Clone the Percona source
WORKDIR /build
RUN git clone --recurse-submodules https://github.com/Percona-Lab/pg_oauth.git pg_oauth


# Build the module using PGXS from the CNPG image
WORKDIR /build/pg_oauth
RUN make USE_PGXS=1 PG_CONFIG=/usr/lib/postgresql/18/bin/pg_config
RUN strip pg_oauth.so


# -----------------------------------------------------------------------------
# Stage 2 — export or package the built .so
# -----------------------------------------------------------------------------
# Option A: minimal image containing only the library (for copying or initContainer)
FROM busybox:uclibc AS export
COPY --from=builder /build/pg_oauth/pg_oauth.so /pg_oauth.so
CMD ["ls", "-lh", "/pg_oauth.so"]
