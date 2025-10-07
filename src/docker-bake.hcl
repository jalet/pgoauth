group "default" {
  targets = ["myimage"]
}

target "myimage" {
  dockerfile-inline = <<EOT
ARG BASE_IMAGE="ghcr.io/cloudnative-pg/postgresql:18.0-system-trixie"
ARG LIBVERSION

FROM ghcr.io/jalet/pgoauth:$LIBVERSION AS lib 

FROM $BASE_IMAGE AS myimage

ARG EXTENSIONS

USER root

RUN apt-get update && \
    apt-get install -y --no-install-recommends $EXTENSIONS && \
    apt-get purge -y --auto-remove -o APT::AutoRemove::RecommendsImportant=false && \
    rm -rf /var/lib/apt/lists/* /var/cache/* /var/log/*

COPY --from=lib pg_oauth.so /usr/lib/postgresql/18/lib/pg_oauth.so

USER 26
EOT
  matrix = {
    tgt = [ "myimage", ]
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
  target = "${tgt}"
  args = {
    BASE_IMAGE = "ghcr.io/cloudnative-pg/postgresql:${cleanVersion(pgVersion)}-system-trixie",
    EXTENSIONS = "${getExtensionsString(pgVersion, extensions)}",
    LIBVERSION = "v0.0.1"
  }
}
