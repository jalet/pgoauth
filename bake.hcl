platforms = [
  "linux/amd64",
  # "linux/arm64"
]

extensions = [ ]

authors = "Joakim Jarsäter <j@jarsater.com>"
url = "https://github.com/jalet/postgres-containers"

variable "DEFAULT_TAG" {
  default = "app:local"
}

// Special target: https://github.com/docker/metadata-action#bake-definition
target "docker-metadata-action" {
  tags = ["${DEFAULT_TAG}"]
}

group "default" {
  targets = ["psql"]
}

target "psql" {
  dockerfile-inline = <<EOT
ARG BASE_IMAGE="ghcr.io/cloudnative-pg/postgresql:18.0-system-trixie"
FROM $BASE_IMAGE AS psql
ARG EXTENSIONS
USER root
RUN apt-get update && \
    apt-get install -y --no-install-recommends $EXTENSIONS && \
    apt-get purge -y --auto-remove -o APT::AutoRemove::RecommendsImportant=false && \
    rm -rf /var/lib/apt/lists/* /var/cache/* /var/log/*
  COPY --from=ghcr.io/jalet/pgoauth:v0.0.1 pg_oauth.so /usr/lib/postgresql/18/lib/pg_oauth.so
USER 26
EOT
  matrix = {
    tgt = [
      "psql"
    ]
    pgVersion = [
      "18.0",
    ]
  }
  platforms = [
   "linux/amd64"
    # "linux/arm64"
  ]
  name = "postgresql-${index(split(".",cleanVersion(pgVersion)),0)}-system-trixie"
  target = "${tgt}"
  args = {
    BASE_IMAGE = "ghcr.io/cloudnative-pg/postgresql:${cleanVersion(pgVersion)}-system-trixie",
    EXTENSIONS = "${getExtensionsString(pgVersion, extensions)}",
  }
}
