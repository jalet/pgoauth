now     = timestamp()
authors = "Joakim Jarsäter <j@jarsater.com>"
url     = "https://github.com/jalet/postgres-containers"

variable "revision" {
  default = ""
}

variable "DEFAULT_TAG" {
  default = "app:local"
}

// Special target: https://github.com/docker/metadata-action#bake-definition
target "docker-metadata-action" {
  tags = ["${DEFAULT_TAG}"]
}


