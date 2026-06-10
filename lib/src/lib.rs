#![allow(non_snake_case, clippy::missing_safety_doc)]

use std::ffi::{c_char, c_int, c_void};
use std::time::{Duration, Instant};

#[cfg(not(test))]
use std::ffi::{CStr, CString};

use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use jsonwebtoken::jwk::JwkSet;
use serde_json::Value;

#[cfg(not(test))]
use std::sync::Once;

// PostgreSQL module magic — required for all loadable PostgreSQL modules.
// Must be the very first thing that runs, establishes ABI compatibility.
// Guard with cfg(not(test)) to avoid linking issues in pure unit tests.
#[cfg(not(test))]
::pgrx::pg_module_magic!();

// ---------------------------------------------------------------------------
// FFI types — mirror the C definitions from pg_oauth_validator.h
// ---------------------------------------------------------------------------

pub const PG_OAUTH_VALIDATOR_MAGIC: u32 = 0x20250220;

#[repr(C)]
pub struct ValidatorModuleState {
    pub sversion:     c_int,
    pub private_data: *mut c_void,
}

#[repr(C)]
pub struct ValidatorModuleResult {
    pub authorized: bool,
    pub authn_id:   *mut c_char,
}

pub type ValidatorStartupCB =
    unsafe extern "C" fn(state: *mut ValidatorModuleState);
pub type ValidatorShutdownCB =
    unsafe extern "C" fn(state: *mut ValidatorModuleState);
pub type ValidatorValidateCB =
    unsafe extern "C" fn(
        state:  *const ValidatorModuleState,
        token:  *const c_char,
        role:   *const c_char,
        result: *mut ValidatorModuleResult,
    ) -> bool;

#[repr(C)]
pub struct OAuthValidatorCallbacks {
    pub magic:       u32,
    pub startup_cb:  Option<ValidatorStartupCB>,
    pub shutdown_cb: Option<ValidatorShutdownCB>,
    pub validate_cb: Option<ValidatorValidateCB>,
}

// SAFETY: PostgreSQL loads this from a single process; the static is
// read-only after initialisation.
unsafe impl Sync for OAuthValidatorCallbacks {}

#[cfg(not(test))]
static CALLBACKS: OAuthValidatorCallbacks = OAuthValidatorCallbacks {
    magic:       PG_OAUTH_VALIDATOR_MAGIC,
    startup_cb:  Some(startup_cb),
    shutdown_cb: Some(shutdown_cb),
    validate_cb: Some(validate_cb),
};

// ---------------------------------------------------------------------------
// GUC storage (raw pointers managed by PostgreSQL's GUC machinery)
// ---------------------------------------------------------------------------

static mut GUC_JWKS_URI:       *mut c_char = std::ptr::null_mut();
static mut GUC_ISSUER:         *mut c_char = std::ptr::null_mut();
static mut GUC_AUDIENCE:       *mut c_char = std::ptr::null_mut();
static mut GUC_REQUIRED_SCOPE: *mut c_char = std::ptr::null_mut();
static mut GUC_SCOPE_CLAIM:    *mut c_char = std::ptr::null_mut();
static mut GUC_ROLES_CLAIM:    *mut c_char = std::ptr::null_mut();
static mut GUC_CACHE_TTL_SECS: i32         = 300;

// ---------------------------------------------------------------------------
// ValidatorConfig — pure-Rust snapshot of configuration (testable)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ValidatorConfig {
    pub jwks_uri:       String,
    pub issuer:         String,
    pub audience:       Option<String>,
    pub required_scope: Option<String>,
    pub scope_claim:    String,
    /// Claim listing the Postgres roles the subject may assume, as a dotted
    /// path (e.g. `realm_access.roles`). When set, the validator only authorizes
    /// a connection whose requested role appears in this claim -- this is the
    /// `delegate_ident_mapping=1` model where the IdP (e.g. Keycloak) drives
    /// which Postgres roles a user may log in as. When `None`, the validator
    /// authenticates only (authn_id = sub) and leaves role mapping to
    /// PostgreSQL's pg_ident.conf / default identity check.
    pub roles_claim:    Option<String>,
    pub cache_ttl:      Duration,
}

// ---------------------------------------------------------------------------
// InternalState — heap-allocated, stored in ValidatorModuleState.private_data
// ---------------------------------------------------------------------------

struct InternalState {
    jwks:       Option<JwkSet>,
    fetched_at: Option<Instant>,
    config:     ValidatorConfig,
}

// ---------------------------------------------------------------------------
// Pure logic — testable without PostgreSQL
// ---------------------------------------------------------------------------

/// Returns true if `required` is empty, or if it appears as a
/// whitespace-delimited token in `token_scopes`.
pub fn check_scope(token_scopes: &str, required: &str) -> bool {
    if required.is_empty() {
        return true;
    }
    token_scopes.split_whitespace().any(|s| s == required)
}

/// Follow a dotted claim path (e.g. `realm_access.roles`) into a JSON value.
fn claim_path<'a>(claims: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = claims;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

/// Extract a list of roles from the claim at `path`. Accepts either a JSON
/// array of strings (`["app_reader","app_writer"]`) or a space-delimited
/// string (`"app_reader app_writer"`). Returns empty if absent or wrong type.
pub fn extract_roles(claims: &Value, path: &str) -> Vec<String> {
    match claim_path(claims, path) {
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(Value::as_str)
            .map(String::from)
            .collect(),
        Some(Value::String(s)) => s.split_whitespace().map(String::from).collect(),
        _ => Vec::new(),
    }
}

/// Decide whether a connection requesting `role` is authorized given the roles
/// granted by the token. Authorized iff `role` is non-empty and present in
/// `granted`. This is the `delegate_ident_mapping=1` check: the IdP's claim is
/// the source of truth for which Postgres roles a subject may assume.
pub fn role_authorized(role: &str, granted: &[String]) -> bool {
    !role.is_empty() && granted.iter().any(|r| r == role)
}

/// Fetch a JWKS document from `uri` and parse it.
pub fn fetch_jwks(uri: &str) -> Result<JwkSet, String> {
    let response = ureq::get(uri)
        .call()
        .map_err(|e| format!("fetch JWKS from {}: {}", uri, e))?;

    let body = response
        .into_string()
        .map_err(|e| format!("read JWKS body: {}", e))?;

    serde_json::from_str::<JwkSet>(&body)
        .map_err(|e| format!("parse JWKS: {}", e))
}

/// Claims extracted from a validated JWT.
#[derive(Debug)]
pub struct TokenClaims {
    pub sub:    Option<String>,
    /// Raw scope string from the configured claim.
    pub scopes: String,
    /// Roles the subject may assume, from the configured `roles_claim`.
    /// Empty when no `roles_claim` is configured or the claim is absent.
    pub roles:  Vec<String>,
}

/// Algorithm allowlist — symmetric and `none` are forbidden.
fn is_allowed_algorithm(alg: Algorithm) -> bool {
    matches!(
        alg,
        Algorithm::RS256
            | Algorithm::RS384
            | Algorithm::RS512
            | Algorithm::PS256
            | Algorithm::PS384
            | Algorithm::PS512
            | Algorithm::ES256
            | Algorithm::ES384
    )
}

/// Validate a bearer token against the provided JWKS and configuration.
///
/// Returns `Ok(TokenClaims)` on success, `Err(description)` on any failure.
pub fn validate_token(
    token:  &str,
    jwks:   &JwkSet,
    config: &ValidatorConfig,
) -> Result<TokenClaims, String> {
    // 1. Decode header to learn alg + optional kid.
    let header = decode_header(token)
        .map_err(|e| format!("decode header: {}", e))?;

    // 2. Enforce algorithm allowlist.
    if !is_allowed_algorithm(header.alg) {
        return Err(format!("algorithm not permitted: {:?}", header.alg));
    }

    // 3. Build a Validation object.
    let mut validation = Validation::new(header.alg);
    validation.set_issuer(&[&config.issuer]);
    validation.validate_exp = true;
    validation.validate_nbf = true;
    validation.leeway = 0;

    match &config.audience {
        Some(aud) => {
            validation.set_audience(&[aud.as_str()]);
        }
        None => {
            validation.validate_aud = false;
        }
    }

    // 4. Locate the right JWK and verify the token.
    let claims: Value = if let Some(kid) = &header.kid {
        // kid present — find matching key or fail.
        let jwk = jwks
            .find(kid)
            .ok_or_else(|| format!("kid '{}' not found in JWKS", kid))?;
        let decoding_key = DecodingKey::from_jwk(jwk)
            .map_err(|e| format!("key decode: {}", e))?;
        decode::<Value>(token, &decoding_key, &validation)
            .map_err(|e| format!("token validation: {}", e))?
            .claims
    } else {
        // No kid — try each key in turn; return first success.
        let mut last_err = String::from("no usable key verified the token");
        let mut found: Option<Value> = None;

        for jwk in &jwks.keys {
            let decoding_key = match DecodingKey::from_jwk(jwk) {
                Ok(k) => k,
                Err(e) => {
                    last_err = format!("key decode: {}", e);
                    continue;
                }
            };
            match decode::<Value>(token, &decoding_key, &validation) {
                Ok(td) => {
                    found = Some(td.claims);
                    break;
                }
                Err(e) => {
                    last_err = format!("token validation: {}", e);
                }
            }
        }

        found.ok_or(last_err)?
    };

    // 5. Extract scope string from the configured claim.
    let scopes = claims
        .get(&config.scope_claim)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    // 6. Extract sub (may be absent).
    let sub = claims
        .get("sub")
        .and_then(Value::as_str)
        .map(String::from);

    // 7. Extract roles from the configured claim (for delegate_ident_mapping).
    let roles = match &config.roles_claim {
        Some(path) => extract_roles(&claims, path),
        None => Vec::new(),
    };

    Ok(TokenClaims { sub, scopes, roles })
}

// ---------------------------------------------------------------------------
// PostgreSQL callbacks — only compiled when not running unit tests
// ---------------------------------------------------------------------------

#[cfg(not(test))]
static GUC_ONCE: Once = Once::new();

/// Read a GUC string pointer; returns `None` for null or empty.
#[cfg(not(test))]
unsafe fn guc_str(ptr: *mut c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let s = CStr::from_ptr(ptr).to_str().unwrap_or("").to_string();
    if s.is_empty() { None } else { Some(s) }
}

#[cfg(not(test))]
unsafe extern "C" fn startup_cb(state: *mut ValidatorModuleState) {
    GUC_ONCE.call_once(|| {
        register_gucs();
    });

    // Read current GUC values and build a config snapshot.
    let jwks_uri = match guc_str(GUC_JWKS_URI) {
        Some(v) => v,
        None => {
            pgrx::error!("pg_oauth.jwks_uri must be set");
        }
    };

    let config = ValidatorConfig {
        jwks_uri,
        issuer:         guc_str(GUC_ISSUER).unwrap_or_default(),
        audience:       guc_str(GUC_AUDIENCE),
        required_scope: guc_str(GUC_REQUIRED_SCOPE),
        scope_claim:    guc_str(GUC_SCOPE_CLAIM).unwrap_or_else(|| "scope".to_string()),
        roles_claim:    guc_str(GUC_ROLES_CLAIM),
        cache_ttl:      Duration::from_secs(GUC_CACHE_TTL_SECS.max(1) as u64),
    };

    let internal = Box::new(InternalState {
        jwks:       None,
        fetched_at: None,
        config,
    });

    (*state).private_data = Box::into_raw(internal) as *mut c_void;
}

#[cfg(not(test))]
unsafe extern "C" fn shutdown_cb(state: *mut ValidatorModuleState) {
    if !(*state).private_data.is_null() {
        drop(Box::from_raw((*state).private_data as *mut InternalState));
        (*state).private_data = std::ptr::null_mut();
    }
}

#[cfg(not(test))]
unsafe extern "C" fn validate_cb(
    state:     *const ValidatorModuleState,
    token_ptr: *const c_char,
    role_ptr:  *const c_char,
    result:    *mut ValidatorModuleResult,
) -> bool {
    // Wrap everything in catch_unwind so we never unwind through C frames in
    // debug / test builds.  (In release panic=abort makes this a no-op.)
    let outcome = std::panic::catch_unwind(|| {
        validate_cb_inner(state, token_ptr, role_ptr, result)
    });
    match outcome {
        Ok(v) => v,
        Err(_) => {
            // Panic: deny access and signal that we handled the call.
            (*result).authorized = false;
            (*result).authn_id   = std::ptr::null_mut();
            false
        }
    }
}

#[cfg(not(test))]
unsafe fn validate_cb_inner(
    state:     *const ValidatorModuleState,
    token_ptr: *const c_char,
    role_ptr:  *const c_char,
    result:    *mut ValidatorModuleResult,
) -> bool {
    // Deny by default.
    (*result).authorized = false;
    (*result).authn_id   = std::ptr::null_mut();

    let internal = &mut *((*state).private_data as *mut InternalState);

    // Refresh JWKS if stale or absent.
    let needs_refresh = internal.jwks.is_none()
        || internal
            .fetched_at
            .map(|t| t.elapsed() >= internal.config.cache_ttl)
            .unwrap_or(true);

    if needs_refresh {
        match fetch_jwks(&internal.config.jwks_uri) {
            Ok(jwks) => {
                internal.jwks       = Some(jwks);
                internal.fetched_at = Some(Instant::now());
            }
            Err(e) => {
                pgrx::warning!("pg_oauth: JWKS fetch failed: {}", e);
                // Return true (we handled the call) but result stays unauthorised.
                return true;
            }
        }
    }

    let jwks = match &internal.jwks {
        Some(j) => j,
        None => return true,
    };

    // Convert token pointer to &str.
    let token = match (|| -> Option<&str> {
        if token_ptr.is_null() { return None; }
        CStr::from_ptr(token_ptr).to_str().ok()
    })() {
        Some(t) => t,
        None => return true,
    };

    // Requested Postgres role (the connection's user=); empty if null.
    let role = if role_ptr.is_null() {
        ""
    } else {
        CStr::from_ptr(role_ptr).to_str().unwrap_or("")
    };

    // Validate the token.
    let claims = match validate_token(token, jwks, &internal.config) {
        Ok(c) => c,
        Err(_e) => {
            return true;
        }
    };

    // Check scope.
    let required = internal
        .config
        .required_scope
        .as_deref()
        .unwrap_or("");

    if !check_scope(&claims.scopes, required) {
        return true;
    }

    // Role authorization (delegate_ident_mapping=1 model). When a roles_claim
    // is configured, the requested role MUST be granted by the token, so the
    // IdP controls which Postgres roles a subject may assume. When no
    // roles_claim is configured, role mapping is left to PostgreSQL
    // (pg_ident.conf / default identity check) and we rely on authn_id alone.
    if internal.config.roles_claim.is_some() && !role_authorized(role, &claims.roles) {
        return true;
    }

    // Authorised — palloc the authn_id string.
    let sub = claims.sub.unwrap_or_default();
    let c_sub = match CString::new(sub) {
        Ok(s) => s,
        Err(_) => return true,
    };
    let bytes = c_sub.as_bytes_with_nul();
    let palloc_ptr = pgrx::pg_sys::palloc(bytes.len()) as *mut c_char;
    std::ptr::copy_nonoverlapping(bytes.as_ptr() as *const c_char, palloc_ptr, bytes.len());

    (*result).authorized = true;
    (*result).authn_id   = palloc_ptr;

    true
}

/// Register all pg_oauth GUC variables with PostgreSQL's GUC machinery.
#[cfg(not(test))]
unsafe fn register_gucs() {
    use pgrx::pg_sys::{
        DefineCustomIntVariable, DefineCustomStringVariable,
        GucContext::PGC_SIGHUP,
    };
    let no_flags: i32 = 0;

    DefineCustomStringVariable(
        c"pg_oauth.jwks_uri".as_ptr(),
        c"URI of the JWKS endpoint for OAuth token validation.".as_ptr(),
        std::ptr::null(),
        &raw mut GUC_JWKS_URI,
        std::ptr::null(),
        PGC_SIGHUP,
        no_flags,
        None,
        None,
        None,
    );

    DefineCustomStringVariable(
        c"pg_oauth.issuer".as_ptr(),
        c"Expected issuer (iss) claim in OAuth tokens.".as_ptr(),
        std::ptr::null(),
        &raw mut GUC_ISSUER,
        std::ptr::null(),
        PGC_SIGHUP,
        no_flags,
        None,
        None,
        None,
    );

    DefineCustomStringVariable(
        c"pg_oauth.audience".as_ptr(),
        c"Expected audience (aud) claim in OAuth tokens.".as_ptr(),
        std::ptr::null(),
        &raw mut GUC_AUDIENCE,
        std::ptr::null(),
        PGC_SIGHUP,
        no_flags,
        None,
        None,
        None,
    );

    DefineCustomStringVariable(
        c"pg_oauth.required_scope".as_ptr(),
        c"Scope that must be present in the token.".as_ptr(),
        std::ptr::null(),
        &raw mut GUC_REQUIRED_SCOPE,
        std::ptr::null(),
        PGC_SIGHUP,
        no_flags,
        None,
        None,
        None,
    );

    DefineCustomStringVariable(
        c"pg_oauth.scope_claim".as_ptr(),
        c"JWT claim name that carries the scope string (default: scope).".as_ptr(),
        std::ptr::null(),
        &raw mut GUC_SCOPE_CLAIM,
        c"scope".as_ptr(),
        PGC_SIGHUP,
        no_flags,
        None,
        None,
        None,
    );

    DefineCustomStringVariable(
        c"pg_oauth.roles_claim".as_ptr(),
        c"Dotted JWT claim path listing the Postgres roles the subject may assume (e.g. realm_access.roles). When set, the requested role must appear in this claim (use with pg_hba delegate_ident_mapping=1). When unset, role mapping is left to PostgreSQL.".as_ptr(),
        std::ptr::null(),
        &raw mut GUC_ROLES_CLAIM,
        std::ptr::null(),
        PGC_SIGHUP,
        no_flags,
        None,
        None,
        None,
    );

    DefineCustomIntVariable(
        c"pg_oauth.jwks_cache_ttl".as_ptr(),
        c"How long (seconds) to cache the JWKS before re-fetching.".as_ptr(),
        std::ptr::null(),
        &raw mut GUC_CACHE_TTL_SECS,
        300,    // boot_val
        1,      // min_val
        86400,  // max_val
        PGC_SIGHUP,
        no_flags,
        None,
        None,
        None,
    );
}

// ---------------------------------------------------------------------------
// Module entry point
// ---------------------------------------------------------------------------

#[cfg(not(test))]
#[no_mangle]
pub extern "C" fn _PG_oauth_validator_module_init() -> *const OAuthValidatorCallbacks {
    &CALLBACKS
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    use jsonwebtoken::{encode, EncodingKey, Header as JwtHeader};
    use serde_json::{json, Value};

    // -----------------------------------------------------------------------
    // Fixture data (loaded at compile time)
    // -----------------------------------------------------------------------

    const RSA_PRIVATE: &str = include_str!("../tests/fixtures/rsa_private.pem");
    const RSA_PUBLIC:  &str = include_str!("../tests/fixtures/rsa_public.pem");
    const EC_PRIVATE:  &str = include_str!("../tests/fixtures/ec_private.pem");
    const EC_PUBLIC:   &str = include_str!("../tests/fixtures/ec_public.pem");
    const JWKS_RSA:    &str = include_str!("../tests/fixtures/jwks_rsa.json");
    const JWKS_EC:     &str = include_str!("../tests/fixtures/jwks_ec.json");

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn now_unix() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    fn valid_claims() -> Value {
        json!({
            "sub":   "alice",
            "iss":   "https://test.example.com/",
            "aud":   "pg",
            "scope": "db:connect",
            "realm_access": { "roles": ["app_reader", "app_writer"] },
            "exp":   now_unix() + 3600,
            "iat":   now_unix(),
        })
    }

    fn test_config() -> ValidatorConfig {
        ValidatorConfig {
            jwks_uri:       "http://unused".to_string(),
            issuer:         "https://test.example.com/".to_string(),
            audience:       Some("pg".to_string()),
            required_scope: Some("db:connect".to_string()),
            scope_claim:    "scope".to_string(),
            roles_claim:    Some("realm_access.roles".to_string()),
            cache_ttl:      Duration::from_secs(300),
        }
    }

    fn mint_rs256(claims: Value, kid: &str) -> String {
        let mut header = JwtHeader::new(Algorithm::RS256);
        header.kid = Some(kid.to_string());
        encode(
            &header,
            &claims,
            &EncodingKey::from_rsa_pem(RSA_PRIVATE.as_bytes()).unwrap(),
        )
        .unwrap()
    }

    fn mint_es256(claims: Value, kid: &str) -> String {
        let mut header = JwtHeader::new(Algorithm::ES256);
        header.kid = Some(kid.to_string());
        encode(
            &header,
            &claims,
            &EncodingKey::from_ec_pem(EC_PRIVATE.as_bytes()).unwrap(),
        )
        .unwrap()
    }

    // -----------------------------------------------------------------------
    // Group 1: check_scope
    // -----------------------------------------------------------------------

    #[test]
    fn scope_exact_match() {
        assert!(check_scope("db:read", "db:read"));
    }

    #[test]
    fn scope_subset_match() {
        assert!(check_scope("db:read db:write", "db:read"));
    }

    #[test]
    fn scope_missing() {
        assert!(!check_scope("db:read", "db:write"));
    }

    #[test]
    fn scope_empty_token() {
        assert!(!check_scope("", "db:connect"));
    }

    #[test]
    fn scope_no_required() {
        assert!(check_scope("anything", ""));
    }

    #[test]
    fn scope_case_sensitive() {
        assert!(!check_scope("db:read", "DB:READ"));
    }

    #[test]
    fn scope_multiple_required_not_supported() {
        // check_scope checks for a single token; "db:read db:write" (with
        // the space) is treated as one required token and will not match
        // any individual token in the space-delimited set.
        assert!(!check_scope("db:read db:write", "db:read db:write"));
    }

    // -----------------------------------------------------------------------
    // Group 2: validate_token with RSA (RS256)
    // -----------------------------------------------------------------------

    #[test]
    fn valid_rs256_accepted() {
        let jwks: JwkSet = serde_json::from_str(JWKS_RSA).unwrap();
        let token = mint_rs256(valid_claims(), "test-rsa-key");
        let result = validate_token(&token, &jwks, &test_config());
        assert!(result.is_ok(), "expected Ok but got: {:?}", result);
    }

    #[test]
    fn expired_token_rejected() {
        let jwks: JwkSet = serde_json::from_str(JWKS_RSA).unwrap();
        let mut claims = valid_claims();
        claims["exp"] = json!(now_unix() - 1);
        let token = mint_rs256(claims, "test-rsa-key");
        assert!(validate_token(&token, &jwks, &test_config()).is_err());
    }

    #[test]
    fn nbf_future_rejected() {
        let jwks: JwkSet = serde_json::from_str(JWKS_RSA).unwrap();
        let mut claims = valid_claims();
        claims["nbf"] = json!(now_unix() + 3600);
        let token = mint_rs256(claims, "test-rsa-key");
        assert!(validate_token(&token, &jwks, &test_config()).is_err());
    }

    #[test]
    fn wrong_issuer_rejected() {
        let jwks: JwkSet = serde_json::from_str(JWKS_RSA).unwrap();
        let mut claims = valid_claims();
        claims["iss"] = json!("https://other.com/");
        let token = mint_rs256(claims, "test-rsa-key");
        assert!(validate_token(&token, &jwks, &test_config()).is_err());
    }

    #[test]
    fn wrong_audience_rejected() {
        let jwks: JwkSet = serde_json::from_str(JWKS_RSA).unwrap();
        let mut claims = valid_claims();
        claims["aud"] = json!("other");
        let token = mint_rs256(claims, "test-rsa-key");
        assert!(validate_token(&token, &jwks, &test_config()).is_err());
    }

    #[test]
    fn missing_sub_gives_none_authn() {
        let jwks: JwkSet = serde_json::from_str(JWKS_RSA).unwrap();
        let mut claims = valid_claims();
        // Remove "sub"
        if let Value::Object(ref mut map) = claims {
            map.remove("sub");
        }
        let token = mint_rs256(claims, "test-rsa-key");
        let result = validate_token(&token, &jwks, &test_config());
        let tc = result.expect("should be Ok even without sub");
        assert!(tc.sub.is_none());
    }

    #[test]
    fn tampered_signature_rejected() {
        let jwks: JwkSet = serde_json::from_str(JWKS_RSA).unwrap();
        let token = mint_rs256(valid_claims(), "test-rsa-key");
        // Replace the last 4 characters of the signature.
        let mut tampered = token.clone();
        let len = tampered.len();
        tampered.truncate(len - 4);
        tampered.push_str("XXXX");
        assert!(validate_token(&tampered, &jwks, &test_config()).is_err());
    }

    #[test]
    fn kid_mismatch_rejected() {
        let jwks: JwkSet = serde_json::from_str(JWKS_RSA).unwrap();
        let token = mint_rs256(valid_claims(), "nonexistent-kid");
        assert!(validate_token(&token, &jwks, &test_config()).is_err());
    }

    // -----------------------------------------------------------------------
    // Group 3: validate_token with ES256
    // -----------------------------------------------------------------------

    #[test]
    fn valid_es256_accepted() {
        let jwks: JwkSet = serde_json::from_str(JWKS_EC).unwrap();
        let token = mint_es256(valid_claims(), "test-ec-key");
        let result = validate_token(&token, &jwks, &test_config());
        assert!(result.is_ok(), "expected Ok but got: {:?}", result);
    }

    #[test]
    fn es256_wrong_key_rejected() {
        // Sign with RSA key but declare ES256 alg — this will produce a
        // structurally invalid ES256 token which the library must reject.
        let _jwks: JwkSet = serde_json::from_str(JWKS_EC).unwrap();
        // Manually build a header claiming ES256 but encode with RSA key.
        // jsonwebtoken::encode will actually enforce the alg/key pairing, so
        // we craft a token whose signature is RSA bytes but whose header says ES256.
        //
        // Easier: mint a valid ES256 token but verify against RSA JWKS.
        let rsa_jwks: JwkSet = serde_json::from_str(JWKS_RSA).unwrap();
        let token = mint_es256(valid_claims(), "test-rsa-key"); // kid exists in RSA JWKS
        // The EC-signed token is presented to the RSA JWKS; RSA key will fail verification.
        assert!(validate_token(&token, &rsa_jwks, &test_config()).is_err());
    }

    // -----------------------------------------------------------------------
    // Group 4: algorithm enforcement
    // -----------------------------------------------------------------------

    #[test]
    fn hs256_rejected() {
        use jsonwebtoken::EncodingKey;
        let secret = b"super-secret-key";
        let header = JwtHeader::new(Algorithm::HS256);
        let token = encode(&header, &valid_claims(), &EncodingKey::from_secret(secret)).unwrap();

        // We verify against RSA JWKS; even before key lookup, alg check should fire.
        let jwks: JwkSet = serde_json::from_str(JWKS_RSA).unwrap();
        let result = validate_token(&token, &jwks, &test_config());
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("algorithm not permitted"),
            "expected 'algorithm not permitted', got: {msg}"
        );
    }

    #[test]
    fn alg_none_rejected() {
        // Manually construct a "alg":"none" JWT.
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

        let header_json = r#"{"alg":"none","typ":"JWT"}"#;
        let claims_json = serde_json::to_string(&valid_claims()).unwrap();

        let h = URL_SAFE_NO_PAD.encode(header_json.as_bytes());
        let c = URL_SAFE_NO_PAD.encode(claims_json.as_bytes());
        let token = format!("{}.{}.", h, c);

        let jwks: JwkSet = serde_json::from_str(JWKS_RSA).unwrap();
        let result = validate_token(&token, &jwks, &test_config());
        assert!(result.is_err(), "alg=none token should be rejected");
    }

    // -----------------------------------------------------------------------
    // Group 5: fetch_jwks with httpmock
    // -----------------------------------------------------------------------

    #[test]
    fn jwks_fetch_success() {
        use httpmock::prelude::*;

        let server = MockServer::start();
        let _mock = server.mock(|when, then| {
            when.method(GET).path("/jwks");
            then.status(200)
                .header("Content-Type", "application/json")
                .body(JWKS_RSA);
        });

        let url = format!("http://{}/jwks", server.address());
        let result = fetch_jwks(&url);
        assert!(result.is_ok(), "expected Ok but got: {:?}", result);
        assert_eq!(result.unwrap().keys.len(), 1);
    }

    #[test]
    fn jwks_fetch_http_error() {
        use httpmock::prelude::*;

        let server = MockServer::start();
        let _mock = server.mock(|when, then| {
            when.method(GET).path("/jwks");
            then.status(503);
        });

        let url = format!("http://{}/jwks", server.address());
        let result = fetch_jwks(&url);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        // ureq surfaces status codes in the error message.
        assert!(
            msg.contains("503") || msg.contains("fetch") || msg.contains("status"),
            "unexpected error message: {msg}"
        );
    }

    #[test]
    fn jwks_fetch_invalid_json() {
        use httpmock::prelude::*;

        let server = MockServer::start();
        let _mock = server.mock(|when, then| {
            when.method(GET).path("/jwks");
            then.status(200)
                .header("Content-Type", "application/json")
                .body("not json");
        });

        let url = format!("http://{}/jwks", server.address());
        let result = fetch_jwks(&url);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("parse"),
            "expected 'parse' in error, got: {msg}"
        );
    }

    #[test]
    fn jwks_fetch_empty_keys() {
        use httpmock::prelude::*;

        let server = MockServer::start();
        let _mock = server.mock(|when, then| {
            when.method(GET).path("/jwks");
            then.status(200)
                .header("Content-Type", "application/json")
                .body(r#"{"keys":[]}"#);
        });

        let url = format!("http://{}/jwks", server.address());
        let result = fetch_jwks(&url);
        assert!(result.is_ok(), "expected Ok but got: {:?}", result);
        assert_eq!(result.unwrap().keys.len(), 0);
    }

    // -----------------------------------------------------------------------
    // Group 6: end-to-end validate_token + check_scope
    // -----------------------------------------------------------------------

    #[test]
    fn e2e_authorized() {
        let jwks: JwkSet = serde_json::from_str(JWKS_RSA).unwrap();
        let token = mint_rs256(valid_claims(), "test-rsa-key");
        let config = test_config();
        let result = validate_token(&token, &jwks, &config).unwrap();
        assert_eq!(result.sub.as_deref(), Some("alice"));
        assert!(check_scope(&result.scopes, "db:connect"));
    }

    #[test]
    fn e2e_wrong_scope_rejected() {
        let jwks: JwkSet = serde_json::from_str(JWKS_RSA).unwrap();
        let token = mint_rs256(valid_claims(), "test-rsa-key");
        let config = test_config();
        let result = validate_token(&token, &jwks, &config).unwrap();
        assert!(!check_scope(&result.scopes, "admin:write"));
    }

    #[test]
    fn e2e_expired_full_flow() {
        let mut claims = valid_claims();
        claims["exp"] = json!(now_unix() - 10);
        let jwks: JwkSet = serde_json::from_str(JWKS_RSA).unwrap();
        let token = mint_rs256(claims, "test-rsa-key");
        let config = test_config();
        assert!(validate_token(&token, &jwks, &config).is_err());
    }

    // -----------------------------------------------------------------------
    // Group 7: role extraction + role authorization (Option C)
    // -----------------------------------------------------------------------

    #[test]
    fn roles_from_array_via_dotted_path() {
        let claims = json!({"realm_access": {"roles": ["app_reader", "app_writer"]}});
        let roles = extract_roles(&claims, "realm_access.roles");
        assert_eq!(roles, vec!["app_reader".to_string(), "app_writer".to_string()]);
    }

    #[test]
    fn roles_from_space_delimited_string() {
        let claims = json!({"roles": "app_reader app_writer"});
        let roles = extract_roles(&claims, "roles");
        assert_eq!(roles, vec!["app_reader".to_string(), "app_writer".to_string()]);
    }

    #[test]
    fn roles_absent_or_wrong_type_is_empty() {
        assert!(extract_roles(&json!({}), "realm_access.roles").is_empty());
        assert!(extract_roles(&json!({"roles": 42}), "roles").is_empty());
    }

    #[test]
    fn role_authorized_requires_membership() {
        let granted = vec!["app_reader".to_string(), "app_writer".to_string()];
        assert!(role_authorized("app_reader", &granted));
        assert!(role_authorized("app_writer", &granted));
        assert!(!role_authorized("dba", &granted));
    }

    #[test]
    fn role_authorized_denies_empty_role() {
        let granted = vec!["app_reader".to_string()];
        assert!(!role_authorized("", &granted));
    }

    #[test]
    fn role_authorized_denies_when_no_roles_granted() {
        assert!(!role_authorized("app_reader", &[]));
    }

    #[test]
    fn validate_token_populates_roles_from_config_claim() {
        let jwks: JwkSet = serde_json::from_str(JWKS_RSA).unwrap();
        let token = mint_rs256(valid_claims(), "test-rsa-key");
        let claims = validate_token(&token, &jwks, &test_config()).unwrap();
        assert!(role_authorized("app_reader", &claims.roles));
        assert!(!role_authorized("dba", &claims.roles));
    }

    #[test]
    fn validate_token_roles_empty_when_no_roles_claim_configured() {
        let jwks: JwkSet = serde_json::from_str(JWKS_RSA).unwrap();
        let token = mint_rs256(valid_claims(), "test-rsa-key");
        let mut config = test_config();
        config.roles_claim = None;
        let claims = validate_token(&token, &jwks, &config).unwrap();
        assert!(claims.roles.is_empty());
    }
}
