ARG BASE_IMAGE="ubuntu:24.04"
FROM $BASE_IMAGE AS builder

ENV DEBIAN_FRONTEND=noninteractive
ENV CC=clang-21
ENV CXX=clang++-21
ENV CXXFLAGS="-O2 -fPIC -stdlib=libc++"
ENV LDFLAGS="-stdlib=libc++"
ENV USE_PGXS=1
ENV PGHOME=/usr/local/pgsql
ENV PATH="${PGHOME}/bin:${PATH}"

# -----------------------------------------------------------------------------
# Stage 1 — build pg_oauth.so with Clang 21 + libc++ (C++23)
# -----------------------------------------------------------------------------


# -----------------------------------------------------------------------------
# 1. Install dependencies and modern Clang toolchain
# -----------------------------------------------------------------------------
RUN apt-get update && apt-get install -y \
    wget curl gnupg software-properties-common lsb-release ca-certificates && \
    wget -nv -O - https://apt.llvm.org/llvm-snapshot.gpg.key | apt-key add - && \
    add-apt-repository "deb http://apt.llvm.org/$(lsb_release -cs)/ llvm-toolchain-$(lsb_release -cs)-21 main" && \
    apt-get update && apt-get install -y \
    clang-21 lld-21 libc++-21-dev libc++abi-21-dev \
    build-essential git pkg-config flex bison \
    libcurl4-openssl-dev libjwt-dev libjansson-dev && \
    rm -rf /var/lib/apt/lists/*

# -----------------------------------------------------------------------------
# 2. Build PostgreSQL 18 locally (headers + pg_config)
# -----------------------------------------------------------------------------
WORKDIR /pgsrc
RUN git clone --depth 1 --branch REL_18_STABLE https://github.com/postgres/postgres.git .
RUN ./configure --prefix=${PGHOME} --enable-cassert && \
    make -j"$(nproc)" && \
    make install && \
    ldconfig

# -----------------------------------------------------------------------------
# 3. Build pg_oauth with PGXS
# -----------------------------------------------------------------------------
WORKDIR /build
RUN git clone --recurse-submodules https://github.com/Percona-Lab/pg_oauth.git
WORKDIR /build/pg_oauth
RUN make -j"$(nproc)" PG_CONFIG=${PGHOME}/bin/pg_config
RUN strip pg_oauth.so

# -----------------------------------------------------------------------------
# 4. Default command — inspect artifact
# -----------------------------------------------------------------------------
CMD ["bash", "-c", "file /build/pg_oauth/pg_oauth.so && ldd /build/pg_oauth/pg_oauth.so"]
