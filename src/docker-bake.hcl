# ── Variables ────────────────────────────────────────────────────────────────

variable "PG_VERSION" {
  default = "18.0"
}

variable "PG_VERSIONS" {
  default = ["18.0"]
}

variable "LIBVERSION" {
  default = "latest"
}

# ── Groups ───────────────────────────────────────────────────────────────────

group "default" {
  targets = ["src-local"]
}

group "src-local-all" {
  targets = [for v in PG_VERSIONS : "src-local-${replace(v, ".", "-")}"]
}

# ── Platforms ────────────────────────────────────────────────────────────────

target "src-all" {
  inherits  = ["src"]
  platforms = ["linux/amd64", "linux/arm64"]
}

# ── lib-inline ────────────────────────────────────────────────────────────────
# Builds pg_oauth.so from local source. Used as a named build context by
# src-local so that no pre-published lib image is required for local builds.

target "lib-inline" {
  dockerfile = "lib/Dockerfile"
  target     = "final"
}

# ── src-local ─────────────────────────────────────────────────────────────────
# Fully self-contained local build. Compiles the lib from local source via the
# lib-inline named context; no remote images or registry access required.
#
# Usage:
#   docker buildx bake -f docker-bake.hcl -f src/docker-bake.hcl src-local

target "src-local" {
  dockerfile-inline = <<EOT
ARG BASE_IMAGE="ghcr.io/cloudnative-pg/postgresql:18.0-system-trixie"

FROM lib AS lib-stage

FROM $BASE_IMAGE AS src
USER root
COPY --from=lib-stage /artifacts/pg_oauth.so /usr/lib/postgresql/18/lib/pg_oauth.so
USER 26
EOT
  contexts = {
    lib = "target:lib-inline"
  }
  args = {
    BASE_IMAGE = "ghcr.io/cloudnative-pg/postgresql:${PG_VERSION}-system-trixie"
  }
  output = ["type=docker"]
  tags   = ["local"]
}

# Matrix target — one image per version in PG_VERSIONS.
# Usage:
#   docker buildx bake -f docker-bake.hcl -f src/docker-bake.hcl src-local-all \
#     --set 'variable.PG_VERSIONS=["18.1","18.2","18.3"]'
target "src-local-matrix" {
  matrix = {
    pg_version = PG_VERSIONS
  }
  name = "src-local-${replace(pg_version, ".", "-")}"
  inherits = ["src-local"]
  args = {
    BASE_IMAGE = "ghcr.io/cloudnative-pg/postgresql:${pg_version}-system-trixie"
  }
  tags = ["local:${pg_version}"]
}

# ── src ───────────────────────────────────────────────────────────────────────
# CI / published build. Pulls the released lib image from ghcr.io.
# Override LIBVERSION to pin a specific digest:
#   docker buildx bake src --set '*.args.LIBVERSION=main-abc1234'

target "src" {
  inherits = ["docker-metadata-action"]
  target   = "src"
  dockerfile-inline = <<EOT
ARG BASE_IMAGE="ghcr.io/cloudnative-pg/postgresql:18.0-system-trixie"
ARG LIBVERSION="latest"

FROM ghcr.io/jalet/postgres-oauth-lib:$LIBVERSION AS lib

FROM $BASE_IMAGE AS src
USER root
COPY --from=lib /artifacts/pg_oauth.so /usr/lib/postgresql/18/lib/pg_oauth.so
USER 26
EOT
  args = {
    BASE_IMAGE = "ghcr.io/cloudnative-pg/postgresql:${PG_VERSION}-system-trixie"
    LIBVERSION = LIBVERSION
  }
}
