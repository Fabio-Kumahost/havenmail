use crate::rate_limit::LoginRateLimiter;
use havenmail_core::auth::jwt::JwtIssuer;
use sqlx::PgPool;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

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
    /// Repo-`config/`-Verzeichnis mit den `*.tera`-Templates
    /// (`HAVENMAIL_CONFIG_DIR`) — für Laufzeit-Rendering von
    /// Security-Settings-Änderungen, siehe routes/security_settings.rs.
    pub config_dir: Arc<PathBuf>,
    /// Serialisiert alle Schreibzugriffe auf dateibasierte Rspamd-/DKIM-
    /// Konfiguration (`security_settings::apply_to_rspamd`,
    /// `dns::apply_dkim_maps`) — ohne diese Sperre könnten zwei gleichzeitige
    /// Requests (Backup lesen → schreiben → configtest → ggf. Backup
    /// zurückspielen) sich verschränken und eine bereits in der DB
    /// gespeicherte Änderung beim Zurückspielen eines älteren Backups
    /// wieder verwerfen (TOCTOU, gefunden im Sicherheits-/Bug-Audit vom
    /// 2026-08-07). Analog zum Postgres-Advisory-Lock-Muster in
    /// `havenmail_core::audit::record`, nur prozesslokal statt DB-weit, da
    /// nur ein API-Prozess je Installation läuft.
    pub mail_config_lock: Arc<Mutex<()>>,
}
