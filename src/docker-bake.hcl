group "default" {
  targets = ["src"]
}

target "src" {
  dockerfile = "Dockerfile"
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
