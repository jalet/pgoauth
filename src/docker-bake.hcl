group "default" {
  targets = ["src"]
}

target "src" {
  dockerfile-inline = <<EOT
ARG BASE_IMAGE="ghcr.io/cloudnative-pg/postgresql:18.0-system-trixie"

FROM $BASE_IMAGE AS psql

ARG EXTENSIONS
ARG LIBVERSION

USER root

RUN apt-get update && \
    apt-get install -y --no-install-recommends $EXTENSIONS && \
    apt-get purge -y --auto-remove -o APT::AutoRemove::RecommendsImportant=false && \
    rm -rf /var/lib/apt/lists/* /var/cache/* /var/log/*

COPY --from=ghcr.io/jalet/pgoauth:$LIBVERSION pg_oauth.so /usr/lib/postgresql/18/lib/pg_oauth.so

USER 26
EOT
  matrix = {
    pgVersion = [ "18.0", ]
  }
  tags = [
    "ghcr.io/jalet/postgres-containers:${pgVersion}"
  ]
  name = "postgresql-${index(split(".",cleanVersion(pgVersion)),0)}-system-trixie"
  platforms = [
    "linux/amd64",
    "linux/arm64",
  ]
  args = {
    BASE_IMAGE = "ghcr.io/cloudnative-pg/postgresql:${cleanVersion(pgVersion)}-system-trixie",
    EXTENSIONS = "${getExtensionsString(pgVersion, extensions)}",
    LIBVERSION = "v0.0.1"
  }
}
