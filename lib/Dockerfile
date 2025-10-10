ARG BASE_IMAGE="ubuntu:24.04"
FROM $BASE_IMAGE AS builder

ENV DEBIAN_FRONTEND=noninteractive
ENV CC=clang-21
ENV CXX=clang++-21
ENV CXXFLAGS="-O2 -fPIC -stdlib=libc++"
ENV LDFLAGS="-stdlib=libc++"

# -----------------------------------------------------------------------------
# 1. Install dependencies and modern Clang toolchain
# -----------------------------------------------------------------------------

# --- add LLVM + PostgreSQL APT repos ----------------------------------------
RUN apt-get update && apt-get install -y wget curl gnupg lsb-release software-properties-common ca-certificates && \
    wget -nv -O - https://apt.llvm.org/llvm-snapshot.gpg.key | apt-key add - && \
    add-apt-repository "deb http://apt.llvm.org/$(lsb_release -cs)/ llvm-toolchain-$(lsb_release -cs)-21 main" && \
    echo "deb http://apt.postgresql.org/pub/repos/apt $(lsb_release -cs)-pgdg main" \
      > /etc/apt/sources.list.d/pgdg.list && \
    curl -fsSL https://www.postgresql.org/media/keys/ACCC4CF8.asc \
      | gpg --dearmor -o /etc/apt/trusted.gpg.d/pgdg.gpg

# --- install all build dependencies -----------------------------------------
RUN apt-get update && apt-get install -y \
      clang-21 lld-21 libc++-21-dev libc++abi-21-dev \
      build-essential pkg-config flex bison git \
      libcurl4-openssl-dev libjwt-dev libjansson-dev libkrb5-dev \
      postgresql-server-dev-18 && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /build
RUN git clone https://github.com/Percona-Lab/pg_oauth.git && \
    rm -rf /build/pg_oauth/jwt-cpp && \
    git clone --depth 1 https://github.com/Thalhammer/jwt-cpp.git /build/pg_oauth/jwt-cpp && \
    git clone --depth 1 https://github.com/kazuho/picojson.git /build/pg_oauth/picojson

WORKDIR /build/pg_oauth
RUN make -j"$(nproc)" USE_PGXS=1 PG_CONFIG=/usr/bin/pg_config CC=clang-21 CXX=clang++-21
RUN strip pg_oauth.so

# -----------------------------------------------------------------------------
# 2. Build pg_oauth with PGXS
# -----------------------------------------------------------------------------

WORKDIR /build
RUN git clone --recurse-submodules https://github.com/Percona-Lab/pg_oauth.git
WORKDIR /build/pg_oauth
RUN make -j"$(nproc)" USE_PGXS=1
RUN strip pg_oauth.so

# -----------------------------------------------------------------------------
# 3. Default command — inspect artifact
# -----------------------------------------------------------------------------
CMD ["bash", "-c", "file /build/pg_oauth/pg_oauth.so && ldd /build/pg_oauth/pg_oauth.so"]
