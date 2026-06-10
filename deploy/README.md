# Deploying pg_oauth to CloudNativePG (extension image volume)

`pg_oauth` is a PostgreSQL 18 OAuth **validator module**. It is delivered to
CloudNativePG 1.29 as an **extension image volume**: a tiny OCI image holding
just the shared library, which CNPG mounts into the Postgres pods. No custom
operand image is required.

Reference: <https://cloudnative-pg.io/docs/1.29/imagevolume_extensions/>

## How it loads (the mechanism)

1. `spec.postgresql.extensions[].image.reference` points at
   `ghcr.io/jalet/pg-oauth`. CNPG mounts it as a Kubernetes `ImageVolume` at
   `/extensions/pg_oauth` and automatically sets:
   - `dynamic_library_path` += `/extensions/pg_oauth/lib`
   - `extension_control_path` += `/extensions/pg_oauth/share` (unused here)
2. `oauth_validator_libraries = 'pg_oauth'` makes PostgreSQL load the validator
   at startup. PostgreSQL resolves the unqualified name `pg_oauth` via
   `load_external_function -> expand_dynamic_library_name -> find_in_dynamic_libpath`,
   searching `dynamic_library_path` and appending `.so` -- so it finds
   `/extensions/pg_oauth/lib/pg_oauth.so`.
3. A `pg_hba.conf` `oauth` rule with `validator=pg_oauth` routes OAuth logins to
   the module.

There is **no `CREATE EXTENSION`** and no SQL/control file: a validator module
is not a SQL extension. The `extensions` entry exists only to mount the image
and wire `dynamic_library_path`.

## The image

Built by `.github/workflows/lib-dkr.yaml` from `lib/`, the image is a `scratch`
image containing only:

```
/lib/pg_oauth.so
/share/extension/        (empty)
```

Multi-arch (amd64+arm64), one image for all 18.x (the `.so` is PG18-ABI-stable).
Tags: `latest` on `main`, the commit `sha`, and semver on `v*.*.*` tags. Pin to a
digest or release tag in production.

## Prerequisites

- **PostgreSQL 18+** (image-volume extensions / `extension_control_path`).
- **Kubernetes >= 1.35** (ImageVolume enabled by default), or **1.33-1.34** with
  the `ImageVolume` feature gate enabled on the API server and kubelets.
- **Container runtime**: containerd >= v2.1.0 or CRI-O >= v1.31.
- The `ghcr.io/jalet/pg-oauth` package must be **public**, or configure an
  image pull secret for the extension image.

## Apply

```bash
kubectl apply -f deploy/cnpg-cluster.yaml
kubectl cnpg status pg-oauth -n databases
# Confirm the path was wired:
kubectl exec -n databases pg-oauth-1 -- psql -tAc 'SHOW dynamic_library_path'
# -> $libdir:/extensions/pg_oauth/lib
```

`oauth_validator_libraries` is a postmaster parameter, so the operator performs a
rolling restart to apply it.

## Authorization model (Option C)

With `delegate_ident_mapping=1`, the validator alone decides which role a token
may assume: the requested Postgres role must appear in the token's
`pg_oauth.roles_claim` (default checks `realm_access.roles`). A token is rejected
outright if it is malformed, uses a non-allowlisted algorithm, fails
signature/expiry/audience validation, or lacks the required scope.

Keycloak controls *which* roles a user may assume; Postgres `GRANT`s control
*what* each role can do.

### Keycloak setup

1. Create realm (or client) roles named exactly like your Postgres roles
   (`app_reader`, `app_writer`, ...) and assign them to users/groups.
2. Realm roles are emitted in `realm_access.roles` by default. For client roles
   use `resource_access.<client>.roles`, or add a Group/Role protocol mapper to
   emit them into whatever claim `pg_oauth.roles_claim` points at.
3. Pre-create the matching Postgres roles via `spec.managed.roles` and grant
   their privileges.

## Caveats

- **`oauth_validator_libraries` + `delegate_ident_mapping=1` must be paired with
  a configured `pg_oauth.roles_claim`.** Without a roles claim under delegate
  mapping, the validator falls back to "authenticated = authorized" and would let
  any valid-token holder log in as any role. The module cannot see the `pg_hba`
  flag, so this is not machine-enforced.
- **Verify on a live CNPG 1.29 cluster.** Delivering a *validator* module via an
  `extensions` entry used only to mount and set `dynamic_library_path` (rather
  than `CREATE EXTENSION` or `shared_preload_libraries`) is slightly off the
  documented happy path.
- **JWKS over HTTPS**: configure `pg_oauth.jwks_uri` with `https://`. Internal
  IdPs with private CAs may need their CA trusted by the validator's TLS stack.
