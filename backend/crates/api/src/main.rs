//! Havenmail Control-Plane API — Binary-Einstiegspunkt.
//!
//! STATUS (M3): + DKIM-Schlüsselerzeugung, DNS-Prüfung/-Empfehlungen.
//! Erfordert `DATABASE_URL`, `HAVENMAIL_JWT_SIGNING_KEY` (mind. 32 Byte),
//! `HAVENMAIL_SECRETS_KEY` (genau 32 Byte) und `HAVENMAIL_HOSTNAME` als
//! Umgebungsvariablen. Siehe docs/architecture.md im Repo-Root.

use havenmail_api::state::AppState;
use havenmail_core::auth::jwt::JwtIssuer;
use std::net::SocketAddr;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_target(false).init();

    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL muss gesetzt sein (siehe .env.example)");
    let signing_key = std::env::var("HAVENMAIL_JWT_SIGNING_KEY")
        .expect("HAVENMAIL_JWT_SIGNING_KEY muss gesetzt sein (siehe .env.example)");
    if signing_key.len() < 32 {
        panic!("HAVENMAIL_JWT_SIGNING_KEY muss mindestens 32 Byte lang sein");
    }
    let secrets_key = std::env::var("HAVENMAIL_SECRETS_KEY")
        .expect("HAVENMAIL_SECRETS_KEY muss gesetzt sein (siehe .env.example)");
    if secrets_key.len() != 32 {
        panic!("HAVENMAIL_SECRETS_KEY muss genau 32 Byte lang sein");
    }
    let mail_hostname = std::env::var("HAVENMAIL_HOSTNAME")
        .expect("HAVENMAIL_HOSTNAME muss gesetzt sein (siehe .env.example)");
    let config_dir = std::env::var("HAVENMAIL_CONFIG_DIR")
        .unwrap_or_else(|_| "/opt/havenmail/config".to_string());

    let db = havenmail_core::db::connect(&database_url)
        .await
        .expect("Datenbankverbindung fehlgeschlagen");
    havenmail_core::db::run_migrations(&db)
        .await
        .expect("Migrationen fehlgeschlagen");
    tracing::info!("Datenbankverbindung hergestellt, Migrationen angewendet");

    let state = AppState {
        db,
        jwt: Arc::new(JwtIssuer::new(signing_key.as_bytes())),
        secrets_key: Arc::new(secrets_key.into_bytes()),
        mail_hostname: Arc::new(mail_hostname),
        login_rate_limiter: Arc::default(),
        config_dir: Arc::new(std::path::PathBuf::from(config_dir)),
        mail_config_lock: Arc::default(),
    };

    let app = havenmail_api::build_router(state);

    let bind_addr: SocketAddr = std::env::var("HAVENMAIL_API_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8080".to_string())
        .parse()
        .expect("HAVENMAIL_API_BIND muss eine gültige host:port-Adresse sein");

    tracing::info!(%bind_addr, "Havenmail API startet");

    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .expect("Konnte nicht an HAVENMAIL_API_BIND binden");

    axum::serve(listener, app).await.expect("Serverfehler");
}
