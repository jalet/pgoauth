# Deploy pg_oauth to a CloudNativePG cluster

Use this guide to add OAuth bearer-token authentication to a CloudNativePG 1.29
cluster by mounting the `pg_oauth` validator as an extension image volume -- no
custom operand image required.

New to `pg_oauth`? Run the
[local quickstart](./getting-started-with-pg-oauth.md) first, then come back.

## Prerequisites

- CloudNativePG operator 1.29+ installed in the cluster.
- Kubernetes >= 1.35 (ImageVolume on by default), or 1.33-1.34 with the
  `ImageVolume` feature gate enabled on the API server and kubelets. Node
  runtime: containerd >= v2.1.0 or CRI-O >= v1.31.
- A PostgreSQL 18 operand image for the cluster's `imageName`.
- The extension image `ghcr.io/jalet/pg-oauth` reachable from the cluster
  (public package, or an image pull secret).
- An OIDC provider reachable over HTTPS from the pods that issues access tokens
  carrying a roles claim (for example Keycloak `realm_access.roles`).
- A libpq 18 client with OAuth support, for verification.

## Step 1: Make the extension image pullable

If `ghcr.io/jalet/pg-oauth` is private, create a pull secret in the cluster's
namespace and reference it on the extension entry:

```bash
kubectl create secret docker-registry ghcr-pull \
  --docker-server=ghcr.io \
  --docker-username=<github-user> \
  --docker-password=<github-token-with-read:packages> \
  -n databases
```

Otherwise make the package public in the GitHub UI and skip the secret.

## Step 2: Emit roles in a token claim

`pg_oauth` authorizes a connection only if the requested Postgres role appears
in the token claim named by `pg_oauth.roles_claim`. Configure your IdP so the
token carries the role names you will use as Postgres roles.

In Keycloak, assign realm roles named exactly like your Postgres roles
(`app_reader`, `app_writer`); they appear in `realm_access.roles` by default.
For client roles, use `resource_access.<client>.roles`, or add a Group/Role
protocol mapper that emits the names into the claim you point `roles_claim` at.

## Step 3: Configure the cluster

Add the extension, the validator GUCs, and a `pg_hba` rule to the `Cluster`
spec. The extension `name` (`pg_oauth`) sets the mount path
(`/extensions/pg_oauth`), which CNPG appends to `dynamic_library_path`;
`oauth_validator_libraries='pg_oauth'` is then resolved along that path.

```yaml
spec:
  imageName: ghcr.io/cloudnative-pg/postgresql:18-standard-trixie
  postgresql:
    extensions:
      - name: pg_oauth
        image:
          reference: ghcr.io/jalet/pg-oauth:latest   # pin a digest in prod
        # imagePullSecrets: [ { name: ghcr-pull } ]   # if private
    parameters:
      oauth_validator_libraries: "pg_oauth"
      pg_oauth.jwks_uri:    "https://idp.example/realms/main/protocol/openid-connect/certs"
      pg_oauth.issuer:      "https://idp.example/realms/main"
      pg_oauth.audience:    "postgres"
      pg_oauth.roles_claim: "realm_access.roles"
    pg_hba:
      - >-
        hostssl all all 0.0.0.0/0 oauth
        issuer="https://idp.example/realms/main"
        scope="openid"
        validator=pg_oauth
        delegate_ident_mapping=1
```

> **Warning:** `delegate_ident_mapping=1` hands role authorization entirely to
> the validator. Always pair it with `pg_oauth.roles_claim`. Without a roles
> claim, the validator authorizes any valid-token holder as any requested role.

If you do not want IdP-driven roles, omit `delegate_ident_mapping=1` and add a
`map=` plus a CNPG-managed `pg_ident` instead; the validator then only
authenticates (`authn_id = sub`) and PostgreSQL maps the identity to a role.

See the [configuration reference](#configuration-reference) for the full GUC
list, and the [CloudNativePG image-volume guide][cnpg-iv] for the `extensions`
and `pg_hba` field specs.

[cnpg-iv]: https://cloudnative-pg.io/docs/1.29/imagevolume_extensions/

## Step 4: Pre-create the login roles

OAuth authenticates a token; it does not create roles. Define every role a token
may assume, and grant its privileges:

```yaml
spec:
  managed:
    roles:
      - name: app_reader
        ensure: present
        login: true
```

## Step 5: Apply and roll out

```bash
kubectl apply -f cluster.yaml
kubectl cnpg status pg-oauth -n databases
```

`oauth_validator_libraries` is a postmaster parameter, so the operator performs
a rolling restart to apply it. Wait for the cluster to report healthy before
verifying.

## Verify

Confirm CNPG wired the library path to the mounted volume:

```bash
kubectl exec -n databases pg-oauth-1 -- psql -tAc 'SHOW dynamic_library_path'
```

Expected output:

```
$libdir:/extensions/pg_oauth/lib
```

Connect with a token as a role the token grants (succeeds), then as a role it
does not grant (denied):

```bash
psql "host=<service> dbname=app user=app_reader sslmode=require \
      oauth_issuer=https://idp.example/realms/main oauth_client_id=<client>"
```

A granted role returns a prompt; an un-granted role fails with
`FATAL: ... authentication failed`.

## Troubleshooting

| Symptom                                              | Cause                                                              | Fix                                                                                  |
| ---------------------------------------------------- | ------------------------------------------------------------------ | ------------------------------------------------------------------------------------ |
| Pod stuck `ContainerCreating`; volume mount error    | ImageVolume unsupported: k8s/runtime too old or feature gate off   | Use k8s >= 1.35, or enable the `ImageVolume` gate; upgrade containerd/CRI-O          |
| `ImagePullBackOff` on the extension image            | Package private, no pull secret                                    | Make the package public, or add `imagePullSecrets` to the extension entry (Step 1)   |
| Operator event rejects `oauth_validator_libraries`   | Treated as a fixed/blocked parameter                               | Check `kubectl describe cluster`; set via the operator's supported mechanism         |
| All OAuth logins fail; logs show the library missing | `dynamic_library_path` not wired or `.so` name mismatch            | Confirm `SHOW dynamic_library_path` includes the mount; ensure the lib is `pg_oauth.so` |
| A valid user is denied a role                        | Requested role absent from the roles claim, wrong claim path, or role not created | Check the token's claim, set `pg_oauth.roles_claim` to its path, create the role     |
| Every login fails with a token error                 | Issuer/audience mismatch, JWKS unreachable, or clock skew          | Align `pg_oauth.issuer`/`audience` with the token; confirm pods can reach the JWKS URI; sync clocks |

## Configuration reference

| GUC                          | Required | Default  | Purpose                                                       |
| ---------------------------- | -------- | -------- | ------------------------------------------------------------- |
| `oauth_validator_libraries`  | yes      | (none)   | Loads the validator; set to `pg_oauth`                        |
| `pg_oauth.jwks_uri`          | yes      | (none)   | JWKS endpoint for token signature keys                        |
| `pg_oauth.issuer`            | yes      | (none)   | Expected `iss` claim                                          |
| `pg_oauth.audience`          | no       | (unchecked) | Expected `aud` claim                                       |
| `pg_oauth.scope_claim`       | no       | `scope`  | Claim carrying the token's scopes                             |
| `pg_oauth.roles_claim`       | for Option C | (none) | Dotted claim path listing assumable roles                   |
| `pg_oauth.jwks_cache_ttl`    | no       | `300`    | Seconds to cache the JWKS                                     |
