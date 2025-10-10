group "default" {
  targets = ["src-local"]
}

target "src-all" {
  inherits  = ["src"]
  platforms = ["linux/amd64", "linux/arm64",]
}

target "src-local" {
  inherits = ["src"]
  output   = ["type=docker"]
  tags     = ["local"]
}

target "src" {
  dockerfile-inline = <<EOT
ARG BASE_IMAGE="ghcr.io/cloudnative-pg/postgresql:18.0-system-trixie"
ARG LIBVERSION="latest"

FROM ghcr.io/jalet/postgres-oauth-lib:$LIBVERSION AS lib 

FROM $BASE_IMAGE AS src

ARG EXTENSIONS

USER root

RUN apt-get update && \
    apt-get install -y --no-install-recommends $EXTENSIONS && \
    apt-get purge -y --auto-remove -o APT::AutoRemove::RecommendsImportant=false && \
    rm -rf /var/lib/apt/lists/* /var/cache/* /var/log/*

COPY --from=lib pg_oauth.so /usr/lib/postgresql/18/lib/pg_oauth.so

# Install runtime dependencies for pg_oauth
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        libcurl4 && \
    rm -rf /var/lib/apt/lists/*

USER 26
  EOT
  matrix = {
    tgt       = [ "src", ]
    pgVersion = [ "18.0", ]
  }
  target = "${tgt}"
  args = {
    BASE_IMAGE = "ghcr.io/cloudnative-pg/postgresql:${cleanVersion(pgVersion)}-system-trixie",
    EXTENSIONS = "${getExtensionsString(pgVersion, extensions)}",
    LIBVERSION = "main-61b665a"
  }
}
