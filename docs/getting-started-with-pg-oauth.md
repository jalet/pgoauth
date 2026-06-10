# Getting started with pg_oauth

This tutorial walks you through running PostgreSQL 18 with OAuth bearer-token
login on your own machine, using a mock identity provider. By the end you will
watch a user log in to Postgres with a token and land in a database role chosen
by that token's claims.

`pg_oauth` is a PostgreSQL 18 OAuth validator module. It checks the bearer token
a client presents at connection time and decides which Postgres role that user
may become. You will run everything in containers, so you do not install
PostgreSQL, Rust, or Python locally.

## What you'll achieve

A local PostgreSQL container that accepts an OAuth token from a mock identity
provider and logs the user in as the role named in the token's roles claim.

## Prerequisites

You need these tools installed and working:

| Tool                 | Minimum version | Check it                   |
| -------------------- | --------------- | -------------------------- |
| Docker Engine        | 24+             | `docker version`           |
| Docker Buildx        | 0.12+           | `docker buildx version`    |
| Docker Compose       | v2              | `docker compose version`   |
| Git                  | any recent      | `git --version`            |

You also need:

- About 4 GB of free disk space.
- Internet access (the steps pull container images and compile the module
  inside a container).
- A free TCP port 5432 on your machine.

> **Note:** You do not need Rust, Python, or a PostgreSQL install. Every build
> and service runs inside a container.

## Step 1: Get the code

Clone the repository and enter it:

```bash
git clone https://github.com/jalet/pgoauth.git
cd pgoauth
```

Confirm you are in the right place:

```bash
ls Makefile lib test deploy
```

Expected output:

```
Makefile

lib:
Dockerfile  docker-bake.hcl  src

test:
docker-compose.yml  gen_token.py  mock_oauth.py  pg_hba.conf  postgresql.conf  run-psql.sh

deploy:
README.md  cnpg-cluster.yaml
```

## Step 2: Build the extension image

This compiles the validator and packages it into the small image that a Postgres
container will load:

```bash
make build
```

The first run pulls a Rust build image and compiles the module, so it takes a
few minutes. When it finishes you see:

```
Staged extension files:
test/ext/lib/pg_oauth.so
```

That file, `test/ext/lib/pg_oauth.so`, is the compiled validator. The build also
created an empty `test/ext/share/extension` directory, which mirrors the layout
CloudNativePG expects.

## Step 3: Verify the build

Confirm the module exports the entry point PostgreSQL looks for:

```bash
make test-symbol
```

Expected output:

```
==> Checking exported symbol...
PASS: symbol present
```

If you see `FAIL: symbol missing`, re-run `make build` and check that Step 2
finished without errors before trying again.

## Step 4: Start Postgres and the mock identity provider

This starts three containers: a mock OAuth server, a JWKS endpoint that serves
its public keys, and a stock PostgreSQL 18 container with the validator mounted
in:

```bash
make up
```

The first run pulls the PostgreSQL image, so allow a minute or two. When it
finishes you see:

```
Waiting for postgres...
Ready.
```

Verify all three containers are healthy:

```bash
docker compose -f test/docker-compose.yml ps --format 'table {{.Name}}\t{{.Status}}'
```

Expected output (each container shows `Up` and `healthy`):

```
NAME                  STATUS
test-jwks-1           Up (healthy)
test-oauth-server-1   Up (healthy)
test-postgres-1       Up (healthy)
```

> **Note:** If `make up` fails with `port is already allocated`, another service
> is using port 5432. Stop it, then run `make down` followed by `make up`.

## Step 5: Log in with an OAuth token

Now the payoff. This requests a token from the mock identity provider and uses
it to connect to Postgres as the role `app_reader`:

```bash
make test-oauth
```

Expected output (timestamp will differ):

```
==> Connecting as app_reader (granted by token realm_access.roles)...
 current_user |              now
--------------+-------------------------------
 app_reader   | 2026-06-10 11:47:52.909796+00
(1 row)

PASS: OAuth login succeeded
```

You logged in to PostgreSQL with an OAuth token. The token's subject is
`testuser`, but `current_user` is `app_reader`, because the token's
`realm_access.roles` claim granted that role. The identity provider, not the
database, decided which role the user may assume.

## Step 6: See authorization at work

A token only grants the roles in its claim. Try logging in as `dba`, a role the
token does not grant:

```bash
make test-oauth-forbidden
```

Expected output:

```
==> Connecting as dba (NOT granted by token realm_access.roles)...
PASS: un-granted role correctly rejected
```

Now confirm an expired token is refused:

```bash
make test-oauth-expired
```

Expected output:

```
==> Connecting with expired token (should be rejected)...
PASS: expired token correctly rejected
```

The validator accepts the granted role, refuses the un-granted role, and rejects
the expired token.

## Step 7: Clean up

Stop and remove the containers and their data:

```bash
make down
```

Expected output ends with the containers being removed:

```
[+] Running 4/4
 ✔ Container test-postgres-1      Removed
 ✔ Container test-oauth-server-1  Removed
 ✔ Container test-jwks-1          Removed
 ✔ Network test_default           Removed
```

## What you built

You ran PostgreSQL 18 that:

1. Loaded the `pg_oauth` validator from a mounted directory (the same mechanism
   CloudNativePG uses).
2. Validated a real JWT against a mock identity provider's keys.
3. Mapped the token's roles claim to a Postgres login role.

## Next steps

- Deploy this to a real CloudNativePG 1.29 cluster. CNPG mounts an extension
  image into the Postgres pods as a Kubernetes ImageVolume, so you reference the
  published `ghcr.io/jalet/pg-oauth` image under `spec.postgresql.extensions`,
  set `oauth_validator_libraries = 'pg_oauth'`, and add an `oauth` rule to
  `pg_hba` with `delegate_ident_mapping=1`. The CloudNativePG guide covers the
  cluster fields: <https://cloudnative-pg.io/docs/1.29/imagevolume_extensions/>.
- Control which roles a token may assume with the `pg_oauth.roles_claim`
  setting, and assign the matching realm roles to users in your identity
  provider so the claim carries them.
