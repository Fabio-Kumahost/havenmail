//! Havenmail Control-Plane API.
//!
//! STATUS (M1): Health-/Readiness-Endpunkte inkl. echtem Datenbank-
//! Konnektivitätscheck; Migrationen (`havenmail-core::db`) werden beim
//! Start ausgeführt, sofern `DATABASE_URL` gesetzt ist. Domain-/Benutzer-
//! verwaltung und die REST-Admin-API folgen in M2 (siehe
//! docs/architecture.md im Repo-Root).

use axum::{extract::State, routing::get, Json, Router};
use serde_json::json;
use sqlx::PgPool;
use std::net::SocketAddr;

#[derive(Clone)]
struct AppState {
    db: Option<PgPool>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_target(false).init();

    let db = match std::env::var("DATABASE_URL") {
        Ok(url) => match havenmail_core::db::connect(&url).await {
            Ok(pool) => {
                if let Err(err) = havenmail_core::db::run_migrations(&pool).await {
                    tracing::error!(%err, "Migrationen fehlgeschlagen — Start wird abgebrochen");
                    std::process::exit(1);
                }
                tracing::info!("Datenbankverbindung hergestellt, Migrationen angewendet");
                Some(pool)
            }
            Err(err) => {
                tracing::error!(%err, "Datenbankverbindung fehlgeschlagen — Start wird abgebrochen");
                std::process::exit(1);
            }
        },
        Err(_) => {
            tracing::warn!(
                "DATABASE_URL nicht gesetzt — API startet ohne Datenbankanbindung (nur für lokale Entwicklung/M0-Kompatibilität)"
            );
            None
        }
    };

    let state = AppState { db };

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .with_state(state);

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

/// Liveness-Probe: Prozess läuft und kann HTTP-Requests annehmen.
async fn healthz() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

/// Readiness-Probe: prüft zusätzlich die tatsächliche Datenbankverbindung,
/// sofern konfiguriert.
async fn readyz(State(state): State<AppState>) -> Json<serde_json::Value> {
    let db_ok = match &state.db {
        Some(pool) => havenmail_core::db::check_connectivity(pool).await,
        None => true, // keine DB konfiguriert -> Check wird nicht als Fehler gewertet (M0-Modus)
    };
    let status = if db_ok { "ready" } else { "not_ready" };
    Json(json!({ "status": status, "checks": { "database": db_ok } }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::http::StatusCode;

    #[tokio::test]
    async fn healthz_returns_ok() {
        let response = healthz().await;
        assert_eq!(response.0["status"], "ok");
    }

    #[tokio::test]
    async fn readyz_without_db_reports_ready() {
        let response = readyz(State(AppState { db: None })).await;
        assert_eq!(response.0["status"], "ready");
        assert_eq!(response.0["checks"]["database"], true);
    }

    #[tokio::test]
    async fn app_healthz_endpoint_responds_200() {
        let app = Router::new().route("/healthz", get(healthz));
        let response = tower::ServiceExt::oneshot(
            app,
            axum::http::Request::builder()
                .uri("/healthz")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(body.as_ref(), br#"{"status":"ok"}"#);
    }
}
