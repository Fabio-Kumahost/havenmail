//! End-to-End-Tests der REST-API gegen eine echte PostgreSQL-Instanz.
//!
//! Brauchen `HAVENMAIL_TEST_DATABASE_URL` (siehe `backend/crates/core/src/db.rs`
//! für dieselbe Konvention) und werden sonst übersprungen. CI setzt die
//! Variable über den Postgres-Service in `.github/workflows/backend.yml`.
//! Jeder Test nutzt eindeutige (UUID-basierte) Domain-/Benutzernamen, damit
//! parallel laufende Tests sich nicht gegenseitig stören.

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use havenmail_api::state::AppState;
use havenmail_core::auth::jwt::JwtIssuer;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn test_database_url() -> Option<String> {
    std::env::var("HAVENMAIL_TEST_DATABASE_URL").ok()
}

async fn setup() -> Option<(Router, PgPool)> {
    let url = test_database_url()?;
    let db = havenmail_core::db::connect(&url)
        .await
        .expect("Verbindung sollte gelingen");
    havenmail_core::db::run_migrations(&db)
        .await
        .expect("Migrationen sollten gelingen");

    let state = AppState {
        db: db.clone(),
        jwt: Arc::new(JwtIssuer::new(b"test-signing-key-at-least-32-bytes!")),
    };
    Some((havenmail_api::build_router(state), db))
}

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(t) = token {
        builder = builder.header("authorization", format!("Bearer {t}"));
    }
    let body = match body {
        Some(v) => Body::from(v.to_string()),
        None => Body::from("{}"),
    };
    let request = builder.body(body).unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json_body: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, json_body)
}

/// Legt einen super_admin-Benutzer direkt per SQL an (Bootstrap-Umgehung,
/// wie es sonst der Installer in M5 tut) und gibt Email + Klartextpasswort
/// zurück.
async fn bootstrap_super_admin(db: &PgPool) -> (String, String) {
    let domain_name = format!("bootstrap-{}.test", Uuid::new_v4());
    let password = "bootstrap-super-secret-pw!";
    let hash = havenmail_core::auth::password::hash_password(password).unwrap();

    sqlx::query("INSERT INTO domains (name) VALUES ($1)")
        .bind(&domain_name)
        .execute(db)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO users (domain_id, local_part, password_hash, role) \
         SELECT id, 'root', $2, 'super_admin' FROM domains WHERE name = $1",
    )
    .bind(&domain_name)
    .bind(&hash)
    .execute(db)
    .await
    .unwrap();

    (format!("root@{domain_name}"), password.to_string())
}

async fn login(app: &Router, email: &str, password: &str) -> String {
    let (status, body) = call(
        app,
        "POST",
        "/api/v1/auth/login",
        None,
        Some(json!({ "email": email, "password": password })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Login sollte gelingen: {body:?}");
    body["access_token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn login_rejects_wrong_password() {
    let Some((app, db)) = setup().await else {
        eprintln!("HAVENMAIL_TEST_DATABASE_URL nicht gesetzt — Test übersprungen");
        return;
    };
    let (email, _password) = bootstrap_super_admin(&db).await;

    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/auth/login",
        None,
        Some(json!({ "email": email, "password": "definitiv-falsch" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_rejects_unknown_user_with_same_status_as_wrong_password() {
    let Some((app, _db)) = setup().await else {
        eprintln!("HAVENMAIL_TEST_DATABASE_URL nicht gesetzt — Test übersprungen");
        return;
    };
    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/auth/login",
        None,
        Some(json!({ "email": "does-not-exist@nowhere.invalid", "password": "irrelevant123456" })),
    )
    .await;
    // Gleicher Status wie falsches Passwort -> keine Unterscheidbarkeit
    // (Schutz vor Benutzer-Enumeration, siehe docs/architecture.md).
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn domain_admin_is_scoped_to_own_domain_and_cannot_see_others() {
    let Some((app, db)) = setup().await else {
        eprintln!("HAVENMAIL_TEST_DATABASE_URL nicht gesetzt — Test übersprungen");
        return;
    };
    let (super_email, super_password) = bootstrap_super_admin(&db).await;
    let super_token = login(&app, &super_email, &super_password).await;

    // Zwei getrennte Domains anlegen.
    let own_domain_name = format!("own-{}.test", Uuid::new_v4());
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/domains",
        Some(&super_token),
        Some(json!({ "name": own_domain_name })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    let own_domain_id = body["id"].as_str().unwrap().to_string();

    let other_domain_name = format!("other-{}.test", Uuid::new_v4());
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/domains",
        Some(&super_token),
        Some(json!({ "name": other_domain_name })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let other_domain_id = body["id"].as_str().unwrap().to_string();

    // Domain-Admin für die eigene Domain anlegen.
    let admin_password = "domain-admin-secret-pw!!";
    let (status, body) = call(
        &app,
        "POST",
        &format!("/api/v1/domains/{own_domain_id}/users"),
        Some(&super_token),
        Some(json!({ "local_part": "admin", "password": admin_password, "role": "domain_admin" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");

    let admin_email = format!("admin@{own_domain_name}");
    let admin_token = login(&app, &admin_email, admin_password).await;

    // domain_admin darf nur die eigene Domain sehen.
    let (status, body) = call(&app, "GET", "/api/v1/domains", Some(&admin_token), None).await;
    assert_eq!(status, StatusCode::OK);
    let domains = body.as_array().unwrap();
    assert_eq!(domains.len(), 1);
    assert_eq!(domains[0]["id"], own_domain_id);

    // Zugriff auf fremde Domain -> 404, nicht 403 (keine Existenz-Bestätigung).
    let (status, _) = call(
        &app,
        "GET",
        &format!("/api/v1/domains/{other_domain_id}"),
        Some(&admin_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // domain_admin darf keinen super_admin anlegen (Rechteausweitung verhindert).
    let (status, _) = call(
        &app,
        "POST",
        &format!("/api/v1/domains/{own_domain_id}/users"),
        Some(&admin_token),
        Some(json!({ "local_part": "wannabe", "password": "irrelevant-1234567", "role": "super_admin" })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn forward_loop_is_rejected() {
    let Some((app, db)) = setup().await else {
        eprintln!("HAVENMAIL_TEST_DATABASE_URL nicht gesetzt — Test übersprungen");
        return;
    };
    let (super_email, super_password) = bootstrap_super_admin(&db).await;
    let super_token = login(&app, &super_email, &super_password).await;

    let domain_name = format!("loops-{}.test", Uuid::new_v4());
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/domains",
        Some(&super_token),
        Some(json!({ "name": domain_name })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let domain_id = body["id"].as_str().unwrap().to_string();

    async fn create_user(app: &Router, token: &str, domain_id: &str, local_part: &str) -> String {
        let (status, body) = call(
            app,
            "POST",
            &format!("/api/v1/domains/{domain_id}/users"),
            Some(token),
            Some(json!({ "local_part": local_part, "password": "some-secure-pw-123456" })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body:?}");
        body["id"].as_str().unwrap().to_string()
    }

    let alice_id = create_user(&app, &super_token, &domain_id, "alice").await;
    let bob_id = create_user(&app, &super_token, &domain_id, "bob").await;

    let bob_email = format!("bob@{domain_name}");
    let alice_email = format!("alice@{domain_name}");

    // alice -> bob: erlaubt
    let (status, body) = call(
        &app,
        "POST",
        &format!("/api/v1/users/{alice_id}/forwards"),
        Some(&super_token),
        Some(json!({ "target_address": bob_email })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");

    // bob -> alice würde die Schleife alice -> bob -> alice schließen: abgelehnt
    let (status, body) = call(
        &app,
        "POST",
        &format!("/api/v1/users/{bob_id}/forwards"),
        Some(&super_token),
        Some(json!({ "target_address": alice_email })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body:?}");

    // Weiterleitung auf die eigene Adresse ebenfalls abgelehnt.
    let (status, _) = call(
        &app,
        "POST",
        &format!("/api/v1/users/{alice_id}/forwards"),
        Some(&super_token),
        Some(json!({ "target_address": alice_email })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
