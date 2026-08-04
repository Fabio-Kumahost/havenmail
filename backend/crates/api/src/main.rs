//! Havenmail Control-Plane API.
//!
//! STATUS (M0): Skeleton. Liefert nur Health-/Readiness-Endpunkte.
//! Domain-/Benutzerverwaltung, Auth/RBAC und die REST-Admin-API folgen in
//! den Meilensteinen M1/M2 (siehe docs/architecture.md im Repo-Root).

use axum::{routing::get, Json, Router};
use serde_json::json;
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_target(false).init();

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz));

    let bind_addr: SocketAddr = std::env::var("HAVENMAIL_API_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8080".to_string())
        .parse()
        .expect("HAVENMAIL_API_BIND muss eine gültige host:port-Adresse sein");

    tracing::info!(%bind_addr, "Havenmail API startet (Skeleton, M0)");

    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .expect("Konnte nicht an HAVENMAIL_API_BIND binden");

    axum::serve(listener, app)
        .await
        .expect("Serverfehler");
}

/// Liveness-Probe: Prozess läuft und kann HTTP-Requests annehmen.
async fn healthz() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

/// Readiness-Probe.
///
/// STATUS (M0): meldet immer "ready", da es noch keine Abhängigkeiten
/// (Datenbank, Config-Rendering) gibt, die geprüft werden müssten. Wird in
/// M1 um einen echten Datenbank-Konnektivitätscheck erweitert.
async fn readyz() -> Json<serde_json::Value> {
    Json(json!({ "status": "ready", "checks": {} }))
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
