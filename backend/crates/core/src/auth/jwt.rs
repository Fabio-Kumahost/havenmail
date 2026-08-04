//! Kurzlebige JWT-Access-Tokens für die Admin-API.
//!
//! Nur HMAC-SHA256 (HS256) über die etablierte `jsonwebtoken`-Crate — keine
//! eigene Signatur-/Verifikationslogik. Refresh-Tokens werden NICHT als JWT
//! ausgestellt, sondern als zufällige, gehashte Opak-Tokens in `sessions`
//! gespeichert (siehe `super::token`), damit sie serverseitig widerrufbar
//! sind; JWTs sind bis zum Ablauf grundsätzlich nicht widerrufbar.

use crate::rbac::Role;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// Lebensdauer eines Access-Tokens. Bewusst kurz (siehe Sicherheitsmodell,
/// docs/architecture.md) — Refresh erfolgt über die widerrufbare Session.
pub const ACCESS_TOKEN_TTL_SECONDS: i64 = 15 * 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject: User-ID
    pub sub: Uuid,
    pub role: Role,
    /// Domain-Scope für domain_admin/user; None = kein Scope-Limit (super_admin)
    pub domain_id: Option<Uuid>,
    /// Issued-at (Unix-Sekunden)
    pub iat: i64,
    /// Expiry (Unix-Sekunden)
    pub exp: i64,
}

#[derive(Debug, Error)]
pub enum JwtError {
    #[error("Token-Erstellung fehlgeschlagen: {0}")]
    Encode(String),
    #[error("Token ungültig oder abgelaufen: {0}")]
    Invalid(String),
}

pub struct JwtIssuer {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
}

impl JwtIssuer {
    /// `signing_key` muss mindestens 32 zufällige Bytes enthalten
    /// (vom Installer generiert, siehe HAVENMAIL_JWT_SIGNING_KEY in .env.example).
    pub fn new(signing_key: &[u8]) -> Self {
        Self {
            encoding_key: EncodingKey::from_secret(signing_key),
            decoding_key: DecodingKey::from_secret(signing_key),
        }
    }

    pub fn issue(
        &self,
        user_id: Uuid,
        role: Role,
        domain_id: Option<Uuid>,
        now_unix: i64,
    ) -> Result<String, JwtError> {
        let claims = Claims {
            sub: user_id,
            role,
            domain_id,
            iat: now_unix,
            exp: now_unix + ACCESS_TOKEN_TTL_SECONDS,
        };
        encode(&Header::default(), &claims, &self.encoding_key)
            .map_err(|e| JwtError::Encode(e.to_string()))
    }

    pub fn verify(&self, token: &str) -> Result<Claims, JwtError> {
        let mut validation = Validation::default();
        validation.set_required_spec_claims(&["exp", "sub"]);
        decode::<Claims>(token, &self.decoding_key, &validation)
            .map(|data| data.claims)
            .map_err(|e| JwtError::Invalid(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issuer() -> JwtIssuer {
        JwtIssuer::new(b"01234567890123456789012345678901")
    }

    fn now_unix() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    #[test]
    fn issues_and_verifies_valid_token() {
        let issuer = issuer();
        let user_id = Uuid::new_v4();
        // `jsonwebtoken` validiert `exp` gegen die tatsächliche Systemzeit,
        // unabhängig vom übergebenen `iat` — daher hier reales "jetzt" statt
        // eines fixen Testzeitstempels.
        let token = issuer
            .issue(user_id, Role::DomainAdmin, None, now_unix())
            .unwrap();
        let claims = issuer.verify(&token).unwrap();
        assert_eq!(claims.sub, user_id);
        assert_eq!(claims.role, Role::DomainAdmin);
        assert_eq!(claims.exp - claims.iat, ACCESS_TOKEN_TTL_SECONDS);
    }

    #[test]
    fn rejects_expired_token() {
        let issuer = issuer();
        // iat weit in der Vergangenheit -> exp ebenfalls abgelaufen
        let token = issuer
            .issue(Uuid::new_v4(), Role::User, None, 1_000_000_000)
            .unwrap();
        assert!(issuer.verify(&token).is_err());
    }

    #[test]
    fn rejects_token_signed_with_different_key() {
        let issuer_a = JwtIssuer::new(b"key-a-key-a-key-a-key-a-key-a-32");
        let issuer_b = JwtIssuer::new(b"key-b-key-b-key-b-key-b-key-b-32");
        let token = issuer_a
            .issue(Uuid::new_v4(), Role::SuperAdmin, None, 1_700_000_000)
            .unwrap();
        assert!(issuer_b.verify(&token).is_err());
    }

    #[test]
    fn rejects_malformed_token() {
        assert!(issuer().verify("not.a.jwt").is_err());
    }
}
