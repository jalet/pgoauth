FROM ubuntu:24.04 AS builder

ENV CC=clang-21
ENV CXX=clang++-21
ENV CXXFLAGS="-stdlib=libc++"
ENV LDFLAGS="-stdlib=libc++"
ENV USE_PGXS=1
ENV PGHOME=/usr/local/pgsql
ENV PATH="${PGHOME}/bin:${PATH}"

# -----------------------------------------------------------------------------
# 1. Install dependencies
# -----------------------------------------------------------------------------
RUN apt-get update && apt-get install -y \
    build-essential pkg-config curl wget gnupg lsb-release ca-certificates git \
    software-properties-common \
    libjwt-dev libcurl4-openssl-dev libssl-dev libreadline-dev zlib1g-dev \
    libxml2-dev libxslt1-dev uuid-dev flex bison

RUN apt-get update && apt-get install -y software-properties-common wget gnupg lsb-release && \
    wget -nv -O - https://apt.llvm.org/llvm-snapshot.gpg.key | apt-key add - && \
    add-apt-repository "deb http://apt.llvm.org/$(lsb_release -cs)/ llvm-toolchain-$(lsb_release -cs)-21 main" && \
    apt-get update && \
    apt-get install -y clang-21 lldb-21 lld-21 libc++-21-dev libc++abi-21-dev

# -----------------------------------------------------------------------------
# 2. Build PostgreSQL 18 from source
# -----------------------------------------------------------------------------
WORKDIR /build
RUN git clone --depth 1 --branch REL_18_STABLE https://github.com/postgres/postgres.git pg-source
WORKDIR /build/pg-source
RUN ./configure \
      --prefix=${PGHOME} \
      --enable-debug \
      --enable-cassert && \
    make -j"$(nproc)" && \
    make install && \
    ldconfig

# -----------------------------------------------------------------------------
# 3. Build pg_oauth against that PostgreSQL
# -----------------------------------------------------------------------------
WORKDIR /build
RUN git clone --recurse-submodules https://github.com/Percona-Lab/pg_oauth.git
WORKDIR /build/pg_oauth
RUN make -j"$(nproc)" PG_CONFIG=${PGHOME}/bin/pg_config


# -----------------------------------------------------------------------------
# 4. Default command: show resulting .so
# -----------------------------------------------------------------------------
FROM alpine:3

LABEL org.opencontainers.image.source=https://github.com/jalet/pgoauth
LABEL org.opencontainers.image.description="pg_oauth.so"
LABEL org.opencontainers.image.licenses=MIT

COPY --from=builder /build/pg_oauth/pg_oauth.so /pg_oauth.so
RUN apk add --no-cache file
CMD ["file", "/pg_oauth.so"]
