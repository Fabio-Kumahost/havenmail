//! PostgreSQL-Verbindungsaufbau und Migrationsausführung.
//!
//! Nutzt `sqlx` (compile-time-geprüfte Queries optional, hier bewusst
//! Runtime-Queries um `cargo build` ohne laufende Datenbank zu erlauben —
//! CI und lokale Entwicklung brauchen sonst zwingend eine Postgres-Instanz
//! beim reinen Kompilieren, was Reproduzierbarkeit erschwert).

use sqlx::postgres::{PgPool, PgPoolOptions};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("Datenbankverbindung fehlgeschlagen: {0}")]
    Connect(#[from] sqlx::Error),
    #[error("Migration fehlgeschlagen: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
}

/// Baut einen Connection-Pool auf. `database_url` z. B.
/// `postgres://havenmail@127.0.0.1/havenmail`.
pub async fn connect(database_url: &str) -> Result<PgPool, DbError> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await?;
    Ok(pool)
}

/// Führt alle Migrationen aus `../../migrations` aus (relativ zu dieser
/// Crate, siehe `backend/migrations/`). Idempotent — bereits angewendete
/// Migrationen werden übersprungen (sqlx führt Buchführung in der Tabelle
/// `_sqlx_migrations`).
pub async fn run_migrations(pool: &PgPool) -> Result<(), DbError> {
    sqlx::migrate!("../../migrations").run(pool).await?;
    Ok(())
}

/// Einfacher Konnektivitätscheck für den Readiness-Endpunkt der API.
pub async fn check_connectivity(pool: &PgPool) -> bool {
    sqlx::query("SELECT 1").execute(pool).await.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Diese Tests brauchen eine laufende Postgres-Instanz und werden nur
    /// ausgeführt, wenn `HAVENMAIL_TEST_DATABASE_URL` gesetzt ist — so
    /// bleibt `cargo test` ohne Datenbank lauffähig (z. B. bei reinen
    /// Auth/RBAC-Änderungen), CI setzt die Variable explizit.
    fn test_database_url() -> Option<String> {
        std::env::var("HAVENMAIL_TEST_DATABASE_URL").ok()
    }

    #[tokio::test]
    async fn migrations_run_cleanly_against_fresh_database() {
        let Some(url) = test_database_url() else {
            eprintln!("HAVENMAIL_TEST_DATABASE_URL nicht gesetzt — Test übersprungen");
            return;
        };
        let pool = connect(&url).await.expect("Verbindung sollte gelingen");
        run_migrations(&pool)
            .await
            .expect("Migrationen sollten sauber durchlaufen");
        assert!(check_connectivity(&pool).await);

        // Migrationen müssen idempotent erneut ausführbar sein (z. B. bei
        // erneutem Installer-/Update-Lauf).
        run_migrations(&pool)
            .await
            .expect("Erneuter Lauf muss idempotent sein");
    }

    #[tokio::test]
    async fn connect_rejects_malformed_url_without_network_access() {
        // Bewusst ein strukturell ungültiges (nicht bloß unerreichbares) URL,
        // damit der Test sofort fehlschlägt statt auf einen Verbindungs-
        // Timeout zu warten.
        let result = connect("not-a-valid-database-url").await;
        assert!(result.is_err());
    }
}
