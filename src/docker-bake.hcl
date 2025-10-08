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
    version = [ "18.0", ]
  }
  platforms = [
    "linux/amd64",
    "linux/arm64",
  ]
  args = {
    BASE_IMAGE = "ghcr.io/cloudnative-pg/postgresql:${cleanVersion(version)}-system-trixie",
    EXTENSIONS = "${getExtensionsString(version, extensions)}",
    LIBVERSION = "main"
  }
}
