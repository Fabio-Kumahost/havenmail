use crate::rate_limit::LoginRateLimiter;
use havenmail_core::auth::jwt::JwtIssuer;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub jwt: Arc<JwtIssuer>,
    /// 32-Byte-Master-Schlüssel für die Verschlüsselung von DKIM-Privatschlüsseln
    /// (`HAVENMAIL_SECRETS_KEY`, vom Installer generiert).
    pub secrets_key: Arc<Vec<u8>>,
    /// Mail-Hostname dieser Installation (`HAVENMAIL_HOSTNAME`), z. B.
    /// `mail.example.org` — Grundlage für MX-/DKIM-DNS-Empfehlungen.
    pub mail_hostname: Arc<String>,
    /// Brute-Force-Schutz für `/api/v1/auth/login` (siehe `rate_limit`-Modul).
    pub login_rate_limiter: Arc<LoginRateLimiter>,
}
