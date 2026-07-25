//! Shared helpers for the integration crate: a throwaway RSA issuer that mints
//! validly-signed tokens and builds a matching [`Validator`]. The JWT/JWKS
//! plumbing mirrors `auth.rs`'s own `#[cfg(test)]` helpers, which the integration
//! crate can't reach. It lives here rather than behind a shipped constructor so
//! the no-toy-store / hexagonal discipline holds — nothing test-only leaks into
//! the library's public surface; these tests use only the real public API
//! (`Validator::from_keys` + `with_allowed_subjects`).

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use jojobot::auth::Validator;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, encode};
use rsa::pkcs1::{EncodeRsaPrivateKey, LineEnding};
use rsa::traits::PublicKeyParts;
use rsa::{RsaPrivateKey, RsaPublicKey};
use serde::Serialize;

pub const ISS: &str = "https://issuer.test";
pub const AUD: &str = "https://resource.test/mcp";
const KID: &str = "test-key-1";

#[derive(Serialize)]
struct Claims {
    sub: String,
    iss: String,
    aud: String,
    exp: u64,
}

/// A throwaway RSA issuer. Holds the signing key and the public `n`/`e`
/// components a JWKS would publish, so the validator it builds decodes from the
/// same material production does.
pub struct TestIdp {
    enc: EncodingKey,
    n: String,
    e: String,
}

impl TestIdp {
    pub fn new() -> Self {
        let mut rng = rand::thread_rng();
        let priv_key = RsaPrivateKey::new(&mut rng, 2048).expect("rsa keygen");
        let pub_key = RsaPublicKey::from(&priv_key);

        let pem = priv_key.to_pkcs1_pem(LineEnding::LF).unwrap();
        let enc = EncodingKey::from_rsa_pem(pem.as_bytes()).unwrap();

        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let n = b64.encode(pub_key.n().to_bytes_be());
        let e = b64.encode(pub_key.e().to_bytes_be());

        Self { enc, n, e }
    }

    /// A validator trusting this issuer's key, bound to `ISS`/`AUD`, carrying the
    /// given subject allowlist — the exact construction path `discover()` uses.
    pub fn validator(&self, allowed: &[&str]) -> Validator {
        let decoding = DecodingKey::from_rsa_components(&self.n, &self.e).unwrap();
        let mut keys = HashMap::new();
        keys.insert(KID.to_string(), decoding);
        Validator::from_keys(ISS, AUD, keys)
            .with_allowed_subjects(allowed.iter().map(|s| s.to_string()))
    }

    /// Mint a validly-signed RS256 token for the given subject id.
    pub fn token(&self, sub: &str) -> String {
        let claims = Claims {
            sub: sub.to_string(),
            iss: ISS.to_string(),
            aud: AUD.to_string(),
            exp: now() + 3600,
        };
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(KID.to_string());
        encode(&header, &claims, &self.enc).unwrap()
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
