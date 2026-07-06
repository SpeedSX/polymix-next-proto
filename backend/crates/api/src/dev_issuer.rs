use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rsa::pkcs1::EncodeRsaPrivateKey;
use rsa::traits::PublicKeyParts;
use rsa::{RsaPrivateKey, RsaPublicKey};

const KID: &str = "dev-key-1";

pub struct DevIssuer {
    encoding_key: jsonwebtoken::EncodingKey,
    kid: String,
    pub jwks_json: serde_json::Value,
}

impl DevIssuer {
    pub fn generate() -> anyhow::Result<Self> {
        let mut rng = rand::thread_rng();
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

        let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
        header.kid = Some(self.kid.clone());

        let token = jsonwebtoken::encode(
            &header,
            &serde_json::Value::Object(claims),
            &self.encoding_key,
        )?;
        Ok(token)
    }
}
