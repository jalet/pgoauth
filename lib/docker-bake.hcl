group "default" {
  targets = ["lib"]
}

target "lib" {
  inherits = ["docker-metadata-action"]
  dockerfile = "lib/Dockerfile"
  tags = [
    "ghcr.io/jalet/pgoauth:${version}"
  ]
  name = "pgoauth-lib"
  matrix = {
    version = ["v0.0.1"]
  }
  platforms = ["linux/amd64", "linux/arm64",]
  attest = [
    "type=provenance,mode=max",
    "type=sbom"
  ]
  annotations = [
    "index,manifest:org.opencontainers.image.created=${now}",
    "index,manifest:org.opencontainers.image.url=${url}",
    "index,manifest:org.opencontainers.image.source=${url}",
    "index,manifest:org.opencontainers.image.version=${version}",
    "index,manifest:org.opencontainers.image.revision=${revision}",
    "index,manifest:org.opencontainers.image.vendor=${authors}",
    "index,manifest:org.opencontainers.image.title=Percona-Lab pg_oauth.so",
    "index,manifest:org.opencontainers.image.description=Experimental OAuth validator library for PostgreSQL 18",
    "index,manifest:org.opencontainers.image.documentation=${url}",
    "index,manifest:org.opencontainers.image.authors=${authors}",
    "index,manifest:org.opencontainers.image.licenses=MIT",
  ]
  labels = {
    "org.opencontainers.image.created" = "${now}",
    "org.opencontainers.image.url" = "${url}",
    "org.opencontainers.image.source" = "${url}",
    "org.opencontainers.image.version" = "${version}",
    "org.opencontainers.image.revision" = "${revision}",
    "org.opencontainers.image.vendor" = "${authors}",
    "org.opencontainers.image.title" = "Percona-Lab pg_oauth.so",
    "org.opencontainers.image.description" = "Experimental OAuth validator library for PostgreSQL 18",
    "org.opencontainers.image.documentation" = "${url}",
    "org.opencontainers.image.authors" = "${authors}",
    "org.opencontainers.image.licenses" = "MIT"
  }
  args = {
    BASE_IMAGE = "ubuntu:24.04",
  }
}
