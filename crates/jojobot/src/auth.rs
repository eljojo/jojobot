//! Resource-server token validation and authorization — the one
//! security-critical module jojobot owns. It never issues tokens; it *validates*
//! bearer JWTs minted by the configured OIDC issuer (per MCP's resource-server
//! model) and then *authorizes* the principal against an optional subject
//! allowlist.
//!
//! Authentication guarantees enforced here:
//! - **RS256 only.** The algorithm is checked against an allowlist before
//!   verification, so `alg: none` and HMAC-confusion tokens are rejected.
//! - **Issuer + audience binding (RFC 8707).** A token minted for a different
//!   resource, or by a different issuer, is rejected.
//! - **Expiry.** Expired tokens are rejected.
//! - **Key pinning by `kid`.** Only keys published in the issuer's JWKS verify a
//!   signature; an unknown `kid` is rejected rather than trusted.
//!
//! Authorization (see [`Validator::authorize`]): when a subject allowlist is
//! configured, a validated token is admitted only if its `sub` is on the list —
//! otherwise 403. With no allowlist, any validated token passes (authentication
//! only).

use std::collections::HashMap;

use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;

use crate::config::AuthConfig;

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("missing bearer token")]
    MissingToken,
    #[error("malformed token: {0}")]
    Malformed(String),
    #[error("unknown signing key id")]
    UnknownKid,
    #[error("disallowed algorithm: {0:?}")]
    DisallowedAlg(Algorithm),
    #[error("token rejected: {0}")]
    Rejected(String),
    /// Authenticated, but the principal is not authorized (subject allowlist).
    /// Distinct from the rejections above so the HTTP layer answers 403, not 401.
    #[error("forbidden: {0}")]
    Forbidden(String),
}

/// The subset of registered claims jojobot reads after validation. Issuer,
/// audience and expiry are validated by [`Validator::validate`] itself; this
/// carries the principal onward.
#[derive(Debug, Clone, Deserialize)]
pub struct Claims {
    /// Subject — the stable, issuer-assigned principal id. This is the
    /// authorization key ([`Validator::authorize`]): unlike `email`, it is
    /// present in Pocket ID access tokens and not user-editable.
    pub sub: String,
    /// OAuth scopes, when present.
    #[serde(default)]
    pub scope: Option<String>,
}

/// Validates bearer tokens against a fixed set of issuer signing keys.
pub struct Validator {
    issuer: String,
    audience: String,
    /// `kid` → decoding key, as published in the issuer's JWKS.
    keys: HashMap<String, DecodingKey>,
    /// Authorization allowlist of subject ids (`sub` claim values). `None` means
    /// no allowlist is configured, so any validated token is authorized
    /// (authentication only).
    allowed_subjects: Option<Vec<String>>,
}

impl Validator {
    /// Construct directly from a keyset. Used by [`Validator::discover`] and by
    /// the golden tests; keeps validation logic independent of the network.
    pub fn from_keys(
        issuer: impl Into<String>,
        audience: impl Into<String>,
        keys: HashMap<String, DecodingKey>,
    ) -> Self {
        Self {
            issuer: issuer.into(),
            audience: audience.into(),
            keys,
            allowed_subjects: None,
        }
    }

    /// Assemble a validator from resolved [`AuthConfig`] and a fetched keyset —
    /// the issuer/audience binding plus the authorization allowlist. Split out of
    /// [`Validator::discover`] so the config→validator wiring (crucially, that the
    /// allowlist is carried through) is unit-testable without the network I/O.
    pub fn from_config(cfg: &AuthConfig, keys: HashMap<String, DecodingKey>) -> Self {
        Self::from_keys(cfg.issuer.clone(), cfg.audience.clone(), keys)
            .with_allowed_subjects(cfg.allowed_subjects.clone())
    }

    /// Set the authorization allowlist of subject ids. Entries are trimmed and
    /// blanks dropped, but NOT case-folded — a `sub` is an opaque, case-sensitive
    /// identifier compared exactly. An empty list leaves the allowlist unset, so
    /// authorization stays open (any validated token passes). Additive: a
    /// validator built without this authorizes every authenticated principal.
    pub fn with_allowed_subjects(mut self, subjects: impl IntoIterator<Item = String>) -> Self {
        let normalized: Vec<String> = subjects
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        self.allowed_subjects = (!normalized.is_empty()).then_some(normalized);
        self
    }

    /// Authorize an authenticated principal against the subject allowlist. Returns
    /// `Ok(())` when no allowlist is configured, or the token's `sub` is on it.
    /// Otherwise fails closed with [`AuthError::Forbidden`]: authentication
    /// succeeded, but the principal is not authorized. `sub` is compared exactly
    /// (case-sensitive) — it is an opaque, issuer-assigned identifier.
    pub fn authorize(&self, claims: &Claims) -> Result<(), AuthError> {
        let Some(allowed) = &self.allowed_subjects else {
            return Ok(());
        };

        if allowed.contains(&claims.sub) {
            Ok(())
        } else {
            Err(AuthError::Forbidden(format!(
                "subject {} is not on the allowlist",
                claims.sub
            )))
        }
    }

    /// Fetch the issuer's JWKS (via OIDC discovery unless an explicit URI is
    /// configured) and build a validator. RSA keys only; a keyless result is an
    /// error rather than a validator that trusts nothing usefully.
    ///
    /// TODO: key rotation is handled by restart only; add live JWKS re-fetch on
    /// an unknown `kid` (with a cooldown) so rotation doesn't need a bounce.
    pub async fn discover(cfg: &AuthConfig, http: &reqwest::Client) -> anyhow::Result<Self> {
        let jwks_uri = match &cfg.jwks_uri {
            Some(uri) => uri.clone(),
            None => {
                let url = format!(
                    "{}/.well-known/openid-configuration",
                    cfg.issuer.trim_end_matches('/')
                );
                let disco: Discovery = http
                    .get(&url)
                    .send()
                    .await?
                    .error_for_status()?
                    .json()
                    .await?;
                disco.jwks_uri
            }
        };

        let jwks: Jwks = http
            .get(&jwks_uri)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let keys = keyset_from_jwks(jwks.keys)?;
        if keys.is_empty() {
            anyhow::bail!("no usable RSA signing keys in JWKS at {jwks_uri}");
        }

        Ok(Self::from_config(cfg, keys))
    }

    /// Validate a raw JWT string. Returns the claims on success.
    pub fn validate(&self, token: &str) -> Result<Claims, AuthError> {
        let header = decode_header(token).map_err(|e| AuthError::Malformed(e.to_string()))?;

        // Allowlist the algorithm *before* touching key material.
        if header.alg != Algorithm::RS256 {
            return Err(AuthError::DisallowedAlg(header.alg));
        }

        let kid = header
            .kid
            .ok_or_else(|| AuthError::Malformed("no kid in header".to_string()))?;
        let key = self.keys.get(&kid).ok_or(AuthError::UnknownKid)?;

        let mut validation = Validation::new(Algorithm::RS256);
        validation.algorithms = vec![Algorithm::RS256];
        validation.set_issuer(&[&self.issuer]);
        validation.set_audience(&[&self.audience]);
        validation.validate_exp = true;
        // Enforced only when the claim is present, so nbf stays optional.
        validation.validate_nbf = true;
        validation.set_required_spec_claims(&["exp", "aud", "iss", "sub"]);

        let data = decode::<Claims>(token, key, &validation)
            .map_err(|e| AuthError::Rejected(e.to_string()))?;
        Ok(data.claims)
    }
}

/// Extract the token from an `Authorization: Bearer …` header value.
pub fn bearer_from_header(value: Option<&str>) -> Result<&str, AuthError> {
    let value = value.ok_or(AuthError::MissingToken)?;
    value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or(AuthError::MissingToken)
}

/// Build the `kid -> decoding key` map from a JWKS document. RSA keys only.
fn keyset_from_jwks(jwks: Vec<Jwk>) -> anyhow::Result<HashMap<String, DecodingKey>> {
    let mut keys = HashMap::new();
    for jwk in jwks {
        if jwk.kty != "RSA" {
            continue;
        }
        // Signing keys only — a key published for encryption must never verify a
        // signature (RFC 7517 §4.3). Absent `use` is treated as usable.
        if jwk.use_.as_deref().is_some_and(|u| u != "sig") {
            continue;
        }
        if let (Some(kid), Some(n), Some(e)) = (jwk.kid, jwk.n, jwk.e) {
            let key = DecodingKey::from_rsa_components(&n, &e)
                .map_err(|err| anyhow::anyhow!("bad RSA key {kid} in JWKS: {err}"))?;
            keys.insert(kid, key);
        }
    }
    Ok(keys)
}

// --- JWKS / discovery wire types (private; ACL for the issuer's JSON) ---

#[derive(Deserialize)]
struct Discovery {
    jwks_uri: String,
}

#[derive(Deserialize)]
struct Jwks {
    keys: Vec<Jwk>,
}

#[derive(Deserialize)]
struct Jwk {
    kty: String,
    #[serde(default)]
    kid: Option<String>,
    #[serde(default)]
    n: Option<String>,
    #[serde(default)]
    e: Option<String>,
    /// Intended key use: "sig" or "enc". A key marked for encryption must not be
    /// trusted for signature verification.
    #[serde(rename = "use", default)]
    use_: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    use base64::Engine;
    use jsonwebtoken::{EncodingKey, Header, encode};
    use rsa::pkcs1::{EncodeRsaPrivateKey, LineEnding};
    use rsa::traits::PublicKeyParts;
    use rsa::{RsaPrivateKey, RsaPublicKey};
    use serde::Serialize;

    const ISS: &str = "https://issuer.test";
    const AUD: &str = "https://jojobot.test/mcp";
    const KID: &str = "test-key-1";

    #[derive(Serialize)]
    struct TestClaims {
        sub: String,
        iss: String,
        aud: String,
        exp: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        nbf: Option<u64>,
    }

    struct KeyPair {
        enc: EncodingKey,
        decoding: DecodingKey,
        n: String,
        e: String,
    }

    /// Generate an RSA keypair and expose it both as a jsonwebtoken signing key
    /// and as a decoding key built the *same way production does* — from the
    /// base64url `n`/`e` components a JWKS would carry.
    fn gen_keypair() -> KeyPair {
        let mut rng = rand::thread_rng();
        let priv_key = RsaPrivateKey::new(&mut rng, 2048).expect("rsa keygen");
        let pub_key = RsaPublicKey::from(&priv_key);

        let pem = priv_key.to_pkcs1_pem(LineEnding::LF).unwrap();
        let enc = EncodingKey::from_rsa_pem(pem.as_bytes()).unwrap();

        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let n = b64.encode(pub_key.n().to_bytes_be());
        let e = b64.encode(pub_key.e().to_bytes_be());
        let decoding = DecodingKey::from_rsa_components(&n, &e).unwrap();

        KeyPair {
            enc,
            decoding,
            n,
            e,
        }
    }

    fn jwk_for(kp: &KeyPair, kid: &str, use_: Option<&str>) -> Jwk {
        Jwk {
            kty: "RSA".to_string(),
            kid: Some(kid.to_string()),
            n: Some(kp.n.clone()),
            e: Some(kp.e.clone()),
            use_: use_.map(str::to_string),
        }
    }

    fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    fn sign(enc: &EncodingKey, kid: &str, claims: &TestClaims) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(kid.to_string());
        encode(&header, claims, enc).unwrap()
    }

    fn validator(decoding: DecodingKey) -> Validator {
        let mut keys = HashMap::new();
        keys.insert(KID.to_string(), decoding);
        Validator::from_keys(ISS, AUD, keys)
    }

    fn good_claims() -> TestClaims {
        TestClaims {
            sub: "user-1".to_string(),
            iss: ISS.to_string(),
            aud: AUD.to_string(),
            exp: now() + 3600,
            nbf: None,
        }
    }

    /// A validator with the given subject allowlist, keyed on `KID`.
    fn allow_validator(decoding: DecodingKey, subjects: &[&str]) -> Validator {
        validator(decoding).with_allowed_subjects(subjects.iter().map(|s| s.to_string()))
    }

    // --- Authorization: the subject allowlist (authn succeeds; authz decides) ---

    #[test]
    fn allowlist_admits_a_listed_subject() {
        let kp = gen_keypair();
        let mut c = good_claims();
        c.sub = "sub-abc".to_string();
        let token = sign(&kp.enc, KID, &c);
        let v = allow_validator(kp.decoding, &["sub-abc", "sub-other"]);
        let claims = v.validate(&token).expect("token is valid");
        assert!(
            v.authorize(&claims).is_ok(),
            "a listed subject must be authorized"
        );
    }

    #[test]
    fn allowlist_rejects_an_unlisted_subject() {
        let kp = gen_keypair();
        let mut c = good_claims();
        c.sub = "sub-stranger".to_string();
        let token = sign(&kp.enc, KID, &c);
        let v = allow_validator(kp.decoding, &["sub-abc"]);
        let claims = v.validate(&token).expect("token is valid");
        assert!(
            matches!(v.authorize(&claims), Err(AuthError::Forbidden(_))),
            "an unlisted subject must be forbidden"
        );
    }

    #[test]
    fn no_allowlist_admits_any_subject() {
        // Unset allowlist preserves authentication-only behaviour.
        let kp = gen_keypair();
        let mut c = good_claims();
        c.sub = "sub-anyone".to_string();
        let token = sign(&kp.enc, KID, &c);
        let v = validator(kp.decoding); // no allowlist
        let claims = v.validate(&token).expect("token is valid");
        assert!(
            v.authorize(&claims).is_ok(),
            "no allowlist must authorize any principal"
        );
    }

    #[test]
    fn allowlist_matches_subject_case_sensitively() {
        // A `sub` is opaque and compared exactly — a case variant must NOT match.
        let kp = gen_keypair();
        let mut c = good_claims();
        c.sub = "SUB-abc".to_string();
        let token = sign(&kp.enc, KID, &c);
        let v = allow_validator(kp.decoding, &["sub-abc"]);
        let claims = v.validate(&token).expect("token is valid");
        assert!(
            matches!(v.authorize(&claims), Err(AuthError::Forbidden(_))),
            "subject matching must be exact/case-sensitive"
        );
    }

    #[test]
    fn allowlist_trims_config_entries() {
        // Whitespace around a configured entry is stripped so it still matches the
        // exact claim; the claim itself is compared as-is.
        let kp = gen_keypair();
        let mut c = good_claims();
        c.sub = "sub-abc".to_string();
        let token = sign(&kp.enc, KID, &c);
        let v = allow_validator(kp.decoding, &["  sub-abc  "]);
        let claims = v.validate(&token).expect("token is valid");
        assert!(
            v.authorize(&claims).is_ok(),
            "a padded config entry must still match"
        );
    }

    #[test]
    fn from_config_carries_the_allowlist_through() {
        // Guards the one production wiring point (discover -> from_config): a
        // regression that drops the allowlist would authorize any principal.
        let kp = gen_keypair();
        let cfg = AuthConfig {
            issuer: ISS.to_string(),
            audience: AUD.to_string(),
            jwks_uri: None,
            allowed_subjects: vec!["sub-abc".to_string()],
        };
        let mut keys = HashMap::new();
        keys.insert(KID.to_string(), kp.decoding);
        let v = Validator::from_config(&cfg, keys);

        let mut c = good_claims();
        c.sub = "sub-stranger".to_string();
        let token = sign(&kp.enc, KID, &c);
        let claims = v.validate(&token).expect("token is valid");
        assert!(
            matches!(v.authorize(&claims), Err(AuthError::Forbidden(_))),
            "from_config must wire the allowlist so an off-list principal is denied"
        );
    }

    #[test]
    fn with_allowed_subjects_empty_stays_open() {
        // The config→validator path for an unset var flows through
        // with_allowed_subjects([]); it must collapse to no-allowlist (allow-all),
        // never to an empty active list that locks everyone out.
        let kp = gen_keypair();
        let token = sign(&kp.enc, KID, &good_claims());
        let v = validator(kp.decoding).with_allowed_subjects(std::iter::empty::<String>());
        let claims = v.validate(&token).expect("token is valid");
        assert!(
            v.authorize(&claims).is_ok(),
            "an empty allowlist must stay open, not lock out"
        );
    }

    #[test]
    fn rejects_not_yet_valid_nbf() {
        let kp = gen_keypair();
        let mut c = good_claims();
        c.nbf = Some(now() + 1800); // activates in 30 min
        let token = sign(&kp.enc, KID, &c);
        assert!(validator(kp.decoding).validate(&token).is_err());
    }

    #[test]
    fn keyset_excludes_non_signing_keys() {
        let sig = gen_keypair();
        let enc = gen_keypair();
        let jwks = vec![
            jwk_for(&sig, "sig-kid", Some("sig")),
            jwk_for(&enc, "enc-kid", Some("enc")),
        ];
        let keys = keyset_from_jwks(jwks).expect("keyset builds");
        assert!(
            keys.contains_key("sig-kid"),
            "the signing key must be usable"
        );
        assert!(
            !keys.contains_key("enc-kid"),
            "an encryption-use key must not be trusted for verification"
        );
    }

    #[test]
    fn accepts_a_well_formed_token() {
        let kp = gen_keypair();
        let token = sign(&kp.enc, KID, &good_claims());
        let claims = validator(kp.decoding)
            .validate(&token)
            .expect("should accept");
        assert_eq!(claims.sub, "user-1");
    }

    #[test]
    fn rejects_wrong_audience() {
        let kp = gen_keypair();
        let mut c = good_claims();
        c.aud = "https://someone-else/mcp".to_string();
        let token = sign(&kp.enc, KID, &c);
        assert!(validator(kp.decoding).validate(&token).is_err());
    }

    #[test]
    fn rejects_wrong_issuer() {
        let kp = gen_keypair();
        let mut c = good_claims();
        c.iss = "https://evil-issuer".to_string();
        let token = sign(&kp.enc, KID, &c);
        assert!(validator(kp.decoding).validate(&token).is_err());
    }

    #[test]
    fn rejects_expired() {
        let kp = gen_keypair();
        let mut c = good_claims();
        // Beyond jsonwebtoken's default 60s clock-skew leeway.
        c.exp = now() - 3600;
        let token = sign(&kp.enc, KID, &c);
        assert!(validator(kp.decoding).validate(&token).is_err());
    }

    #[test]
    fn rejects_unknown_kid() {
        let kp = gen_keypair();
        let token = sign(&kp.enc, "some-other-kid", &good_claims());
        assert!(matches!(
            validator(kp.decoding).validate(&token),
            Err(AuthError::UnknownKid)
        ));
    }

    #[test]
    fn rejects_hmac_algorithm_confusion() {
        // A token signed with HS256 must never be accepted by an RS256 verifier.
        let kp = gen_keypair();
        let hmac = EncodingKey::from_secret(b"a-shared-secret");
        let mut header = Header::new(Algorithm::HS256);
        header.kid = Some(KID.to_string());
        let token = encode(&header, &good_claims(), &hmac).unwrap();
        assert!(matches!(
            validator(kp.decoding).validate(&token),
            Err(AuthError::DisallowedAlg(Algorithm::HS256))
        ));
    }

    #[test]
    fn rejects_tampered_signature() {
        let kp = gen_keypair();
        let token = sign(&kp.enc, KID, &good_claims());
        // Flip the last character of the signature segment.
        let mut chars: Vec<char> = token.chars().collect();
        let last = chars.len() - 1;
        chars[last] = if chars[last] == 'A' { 'B' } else { 'A' };
        let tampered: String = chars.into_iter().collect();
        assert!(validator(kp.decoding).validate(&tampered).is_err());
    }

    #[test]
    fn rejects_signature_from_a_different_key() {
        // Signed by key B, verified against key A's public material.
        let signer = gen_keypair();
        let other = gen_keypair();
        let token = sign(&signer.enc, KID, &good_claims());
        assert!(validator(other.decoding).validate(&token).is_err());
    }

    #[test]
    fn parses_bearer_header() {
        assert_eq!(
            bearer_from_header(Some("Bearer abc.def.ghi")).unwrap(),
            "abc.def.ghi"
        );
        assert_eq!(bearer_from_header(Some("bearer abc")).unwrap(), "abc");
        assert!(bearer_from_header(None).is_err());
        assert!(bearer_from_header(Some("Basic xyz")).is_err());
        assert!(bearer_from_header(Some("Bearer   ")).is_err());
    }
}
