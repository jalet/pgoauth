# -----------------------------------------------------------------------------
# Standalone builder for pg_oauth.so (C++23, Clang 21, libc++)
# Uses prebuilt PostgreSQL 18 dev headers instead of compiling Postgres
# -----------------------------------------------------------------------------
FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive
ENV CC=clang-21
ENV CXX=clang++-21
ENV CXXFLAGS="-O2 -fPIC -stdlib=libc++"
ENV LDFLAGS="-stdlib=libc++"
ENV USE_PGXS=1
ENV PATH="/usr/lib/postgresql/18/bin:${PATH}"

# -----------------------------------------------------------------------------
# 1. Install Clang 21 toolchain and deps
# -----------------------------------------------------------------------------
RUN apt-get update && apt-get install -y \
    wget curl gnupg software-properties-common lsb-release ca-certificates && \
    wget -nv -O - https://apt.llvm.org/llvm-snapshot.gpg.key | apt-key add - && \
    add-apt-repository "deb http://apt.llvm.org/$(lsb_release -cs)/ llvm-toolchain-$(lsb_release -cs)-21 main" && \
    apt-get update && apt-get install -y \
    clang-21 lld-21 libc++-21-dev libc++abi-21-dev && \
    rm -rf /var/lib/apt/lists/*

# -----------------------------------------------------------------------------
# 2. Add PostgreSQL official APT repo and install headers
# -----------------------------------------------------------------------------
RUN echo "deb http://apt.postgresql.org/pub/repos/apt $(lsb_release -cs)-pgdg main" \
      > /etc/apt/sources.list.d/pgdg.list && \
    curl -fsSL https://www.postgresql.org/media/keys/ACCC4CF8.asc | gpg --dearmor -o /etc/apt/trusted.gpg.d/pgdg.gpg && \
    apt-get update && apt-get install -y \
        postgresql-server-dev-18 \
        build-essential git pkg-config flex bison \
        libcurl4-openssl-dev libjwt-dev libjansson-dev && \
    rm -rf /var/lib/apt/lists/*

# -----------------------------------------------------------------------------
# 3. Build pg_oauth with PGXS
# -----------------------------------------------------------------------------
WORKDIR /build
RUN git clone --recurse-submodules https://github.com/Percona-Lab/pg_oauth.git
WORKDIR /build/pg_oauth
RUN make -j"$(nproc)" USE_PGXS=1 PG_CONFIG=/usr/bin/pg_config
RUN strip pg_oauth.so

# -----------------------------------------------------------------------------
# 4. Output artifact info
# -----------------------------------------------------------------------------
CMD ["bash", "-c", "file /build/pg_oauth/pg_oauth.so && ldd /build/pg_oauth/pg_oauth.so"]
