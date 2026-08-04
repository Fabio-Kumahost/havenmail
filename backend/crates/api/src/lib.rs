//! Havenmail Control-Plane API — Bibliotheksteil.
//!
//! Als Lib+Bin aufgeteilt, damit Integrationstests (`tests/`) den echten
//! Router gegen eine reale PostgreSQL-Instanz ausführen können, statt nur
//! einzelne Handler isoliert zu testen.

pub mod auth_extractor;
pub mod error;
pub mod routes;
pub mod state;

use axum::{extract::State, routing::get, Json, Router};
use serde_json::json;
use state::AppState;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .merge(routes::router())
        .with_state(state)
}

/// Liveness-Probe: Prozess läuft und kann HTTP-Requests annehmen.
async fn healthz() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

/// Readiness-Probe: prüft die tatsächliche Datenbankverbindung.
async fn readyz(State(state): State<AppState>) -> Json<serde_json::Value> {
    let db_ok = havenmail_core::db::check_connectivity(&state.db).await;
    let status = if db_ok { "ready" } else { "not_ready" };
    Json(json!({ "status": status, "checks": { "database": db_ok } }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn healthz_returns_ok() {
        let response = healthz().await;
        assert_eq!(response.0["status"], "ok");
    }
}
