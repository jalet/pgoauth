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
}

target "src" {
  dockerfile = "Dockerfile"
  matrix = {
    tgt       = [ "src", ]
    pgVersion = [ "18.0", ]
  }
  target = "${tgt}"
  platforms = [
    "linux/amd64",
    "linux/arm64",
  ]
  args = {
    BASE_IMAGE = "ghcr.io/cloudnative-pg/postgresql:${cleanVersion(pgVersion)}-system-trixie",
    EXTENSIONS = "${getExtensionsString(pgVersion, extensions)}",
    LIBVERSION = "main"
  }
}
