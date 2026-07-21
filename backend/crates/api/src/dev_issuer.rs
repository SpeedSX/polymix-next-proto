use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::SeedableRng;
use rand::rngs::StdRng;
use rsa::pkcs1::EncodeRsaPrivateKey;
use rsa::traits::PublicKeyParts;
use rsa::{RsaPrivateKey, RsaPublicKey};

const KID: &str = "dev-key-1";

/// Fixed seed for the dev signing key. Deriving the keypair deterministically
/// (rather than from entropy) means every server restart re-creates the same
/// key, so dev tokens already cached in the browser keep validating instead of
/// being silently invalidated on each restart. Dev-only — never used for real
/// tenants.
const DEV_KEY_SEED: u64 = 0x504f_4c59_4d49_5800;

pub struct DevIssuer {
    encoding_key: jsonwebtoken::EncodingKey,
    kid: String,
    pub jwks_json: serde_json::Value,
}

impl DevIssuer {
    pub fn generate() -> anyhow::Result<Self> {
        let mut rng = StdRng::seed_from_u64(DEV_KEY_SEED);
        let private_key = RsaPrivateKey::new(&mut rng, 2048)?;
        let public_key = RsaPublicKey::from(&private_key);

        let pem = private_key.to_pkcs1_pem(rsa::pkcs1::LineEnding::LF)?;
        let encoding_key = jsonwebtoken::EncodingKey::from_rsa_pem(pem.as_bytes())?;

        let n = URL_SAFE_NO_PAD.encode(public_key.n().to_bytes_be());
        let e = URL_SAFE_NO_PAD.encode(public_key.e().to_bytes_be());

        let jwks_json = serde_json::json!({
            "keys": [{
                "kty": "RSA",
                "use": "sig",
                "kid": KID,
                "alg": "RS256",
                "n": n,
                "e": e,
            }]
        });

        Ok(Self {
            encoding_key,
            kid: KID.to_string(),
            jwks_json,
        })
    }

    pub fn issue_token(
        &self,
        issuer: &str,
        org_claim: &str,
        user_id: &str,
        org_id: &str,
    ) -> anyhow::Result<String> {
        let now = chrono::Utc::now();
        let exp = (now + chrono::Duration::hours(24)).timestamp();

        let mut claims = serde_json::Map::new();
        claims.insert("iss".into(), serde_json::Value::String(issuer.to_string()));
        claims.insert("sub".into(), serde_json::Value::String(user_id.to_string()));
        claims.insert("exp".into(), serde_json::Value::from(exp));
        claims.insert(
            org_claim.to_string(),
            serde_json::Value::String(org_id.to_string()),
        );

        self.issue_token_with_claims(serde_json::Value::Object(claims))
    }

    pub fn issue_token_with_claims(&self, claims: serde_json::Value) -> anyhow::Result<String> {
        let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
        header.kid = Some(self.kid.clone());

        let token = jsonwebtoken::encode(&header, &claims, &self.encoding_key)?;
        Ok(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh issuer instance (as after a server restart) publishes the same
    /// public key material, so a token minted before the restart still
    /// validates against the new JWKS.
    #[test]
    fn key_material_is_stable_across_instances() {
        let first = DevIssuer::generate().unwrap();
        let second = DevIssuer::generate().unwrap();

        assert_eq!(first.jwks_json, second.jwks_json);
    }
}
