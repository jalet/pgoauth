# pg_oauth

A PostgreSQL 18 OAuth validator module, delivered as a CloudNativePG 1.29
extension image volume. It validates OIDC bearer tokens (JWTs) at connection
time and maps a token claim to the Postgres role the user logs in as -- so your
identity provider (e.g. Keycloak) controls database access. No custom Postgres
image required.

## Highlights

- Verifies JWT signature, issuer, audience, expiry/nbf, and required scope
  (RS/PS/ES algorithms; `alg=none` and symmetric algorithms rejected).
- Maps a roles claim (e.g. `realm_access.roles`) to Postgres roles via
  `pg_hba` `delegate_ident_mapping=1`.
- Ships as a small OCI image (`ghcr.io/jalet/pg-oauth`) that CloudNativePG
  mounts into the Postgres pods -- found via `dynamic_library_path`, no
  `CREATE EXTENSION`.

## Try it locally

```bash
make up           # builds the extension, starts a mock IdP + Postgres
make test-oauth   # logs in with a token as a claim-granted role
make down
```

## Documentation

- [Getting started](docs/getting-started-with-pg-oauth.md) -- hands-on local
  tutorial.
- [Deploy to CloudNativePG](docs/deploy-pg-oauth-to-cloudnativepg.md) -- cluster
  how-to, IdP setup, and troubleshooting.

## Layout

| Path     | Contents                                               |
| -------- | ------------------------------------------------------ |
| `lib/`   | Rust validator crate and its container image           |
| `test/`  | docker-compose harness: mock IdP, JWKS, Postgres       |
| `docs/`  | Tutorial and deployment how-to                         |
| `Makefile` | `build`, `up`, `unit-test`, `test-oauth`, `test`     |

## License

MIT. See [LICENSE](LICENSE).
