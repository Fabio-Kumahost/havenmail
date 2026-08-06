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
        secrets_key: Arc::new(vec![3u8; 32]),
        mail_hostname: Arc::new("mail.havenmail-test.invalid".to_string()),
        login_rate_limiter: Arc::default(),
        config_dir: Arc::new(std::path::PathBuf::from(
            env!("CARGO_MANIFEST_DIR").to_string() + "/../../../config",
        )),
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

#[tokio::test]
async fn dkim_generation_updates_dns_recommendations() {
    let Some((app, db)) = setup().await else {
        eprintln!("HAVENMAIL_TEST_DATABASE_URL nicht gesetzt — Test übersprungen");
        return;
    };
    let (super_email, super_password) = bootstrap_super_admin(&db).await;
    let super_token = login(&app, &super_email, &super_password).await;

    let domain_name = format!("dkim-{}.test", Uuid::new_v4());
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

    // Vor der Schlüsselerzeugung: Empfehlung ohne DKIM-Eintrag.
    let (status, body) = call(
        &app,
        "GET",
        &format!("/api/v1/domains/{domain_id}/dns-recommendations"),
        Some(&super_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["dkim"].is_null());
    assert!(body["spf"]["value"].as_str().unwrap().contains("v=spf1"));

    let (status, body) = call(
        &app,
        "POST",
        &format!("/api/v1/domains/{domain_id}/dkim"),
        Some(&super_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert!(body["dns_record_value"]
        .as_str()
        .unwrap()
        .starts_with("v=DKIM1"));

    // Nach der Schlüsselerzeugung: Empfehlung enthält den echten DKIM-Eintrag.
    let (status, body) = call(
        &app,
        "GET",
        &format!("/api/v1/domains/{domain_id}/dns-recommendations"),
        Some(&super_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["dkim"]["value"]
        .as_str()
        .unwrap()
        .starts_with("v=DKIM1"));
}

#[tokio::test]
async fn system_status_requires_super_admin_and_reports_database_up() {
    let Some((app, db)) = setup().await else {
        eprintln!("HAVENMAIL_TEST_DATABASE_URL nicht gesetzt — Test übersprungen");
        return;
    };
    let (super_email, super_password) = bootstrap_super_admin(&db).await;
    let super_token = login(&app, &super_email, &super_password).await;

    let (status, body) = call(
        &app,
        "GET",
        "/api/v1/system/status",
        Some(&super_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert_eq!(body["database"], json!(true));
    let services = body["services"].as_array().expect("services ist ein Array");
    assert!(services.iter().any(|s| s["unit"] == "postfix"));
    // `tls` hängt vom Dateisystem außerhalb der Testkontrolle ab
    // (${HAVENMAIL_ETC_DIR:-/etc/havenmail}/tls-expiry): in CI/lokal ohne
    // install.sh-Lauf fehlt die Datei -> null; läuft der Test dagegen direkt
    // auf einem echten Havenmail-Host, existiert sie echt. Beide sind
    // korrektes Verhalten der Route, also beide hier zulässig — nur die
    // Form im nicht-null-Fall wird geprüft, nicht welcher Fall es ist.
    assert!(
        body["tls"].is_null() || body["tls"]["expires_at"].is_string(),
        "tls sollte entweder null oder ein Objekt mit expires_at sein: {:?}",
        body["tls"]
    );

    // Ohne Token: nicht authentifiziert, keine Statusdetails preisgegeben.
    let (status, _) = call(&app, "GET", "/api/v1/system/status", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn audit_log_records_domain_mutations_and_scopes_domain_admin() {
    let Some((app, db)) = setup().await else {
        eprintln!("HAVENMAIL_TEST_DATABASE_URL nicht gesetzt — Test übersprungen");
        return;
    };
    let (super_email, super_password) = bootstrap_super_admin(&db).await;
    let super_token = login(&app, &super_email, &super_password).await;

    // Domain anlegen -> erzeugt einen "domain.create"-Audit-Eintrag.
    let domain_name = format!("audit-{}.test", Uuid::new_v4());
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/domains",
        Some(&super_token),
        Some(json!({ "name": domain_name })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    let domain_id = body["id"].as_str().unwrap().to_string();

    // Eine zweite, fremde Domain für den Scoping-Teil des Tests.
    let other_domain_name = format!("audit-other-{}.test", Uuid::new_v4());
    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/domains",
        Some(&super_token),
        Some(json!({ "name": other_domain_name })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // super_admin sieht den domain.create-Eintrag, gefiltert auf die erste Domain.
    let (status, body) = call(
        &app,
        "GET",
        &format!("/api/v1/audit-log?domain_id={domain_id}"),
        Some(&super_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    let entries = body.as_array().expect("audit-log ist ein Array");
    assert!(entries
        .iter()
        .any(|e| e["action"] == "domain.create" && e["target"] == domain_id));
    assert!(entries.iter().all(|e| e["domain_id"] == domain_id));

    // domain_admin für die erste Domain anlegen und einloggen.
    let admin_password = "audit-domain-admin-pw!!";
    let (status, body) = call(
        &app,
        "POST",
        &format!("/api/v1/domains/{domain_id}/users"),
        Some(&super_token),
        Some(json!({ "local_part": "admin", "password": admin_password, "role": "domain_admin" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    let admin_email = format!("admin@{domain_name}");
    let admin_token = login(&app, &admin_email, admin_password).await;

    // domain_admin sieht nur Einträge der eigenen Domain (domain_id-Query wird ignoriert).
    let (status, body) = call(&app, "GET", "/api/v1/audit-log", Some(&admin_token), None).await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    let entries = body.as_array().expect("audit-log ist ein Array");
    assert!(!entries.is_empty());
    assert!(entries.iter().all(|e| e["domain_id"] == domain_id));
    assert!(entries
        .iter()
        .any(|e| e["action"] == "user.create" && e["domain_id"] == domain_id));

    // Kein Token -> nicht authentifiziert.
    let (status, _) = call(&app, "GET", "/api/v1/audit-log", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn audit_log_covers_aliases_distribution_lists_and_dkim() {
    let Some((app, db)) = setup().await else {
        eprintln!("HAVENMAIL_TEST_DATABASE_URL nicht gesetzt — Test übersprungen");
        return;
    };
    let (super_email, super_password) = bootstrap_super_admin(&db).await;
    let super_token = login(&app, &super_email, &super_password).await;

    let domain_name = format!("audit-alias-{}.test", Uuid::new_v4());
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/domains",
        Some(&super_token),
        Some(json!({ "name": domain_name })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    let domain_id = body["id"].as_str().unwrap().to_string();

    let (status, body) = call(
        &app,
        "POST",
        &format!("/api/v1/domains/{domain_id}/aliases"),
        Some(&super_token),
        Some(json!({ "source": format!("info@{domain_name}"), "destinations": [format!("admin@{domain_name}")] })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    let alias_id = body["id"].as_str().unwrap().to_string();

    let (status, body) = call(
        &app,
        "POST",
        &format!("/api/v1/domains/{domain_id}/distribution-lists"),
        Some(&super_token),
        Some(json!({ "address": format!("team@{domain_name}"), "members": [format!("admin@{domain_name}")] })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");

    let (status, body) = call(
        &app,
        "POST",
        &format!("/api/v1/domains/{domain_id}/dkim"),
        Some(&super_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");

    let (status, _) = call(
        &app,
        "DELETE",
        &format!("/api/v1/aliases/{alias_id}"),
        Some(&super_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = call(
        &app,
        "GET",
        &format!("/api/v1/audit-log?domain_id={domain_id}"),
        Some(&super_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    let entries = body.as_array().expect("audit-log ist ein Array");
    for expected_action in [
        "alias.create",
        "alias.delete",
        "distribution_list.create",
        "dkim.generate",
    ] {
        assert!(
            entries.iter().any(|e| e["action"] == expected_action),
            "erwartete Aktion '{expected_action}' fehlt im Audit-Log: {entries:?}"
        );
    }
}

#[tokio::test]
async fn refresh_rotates_tokens_and_revokes_the_old_refresh_token() {
    let Some((app, db)) = setup().await else {
        eprintln!("HAVENMAIL_TEST_DATABASE_URL nicht gesetzt — Test übersprungen");
        return;
    };
    let (email, password) = bootstrap_super_admin(&db).await;

    let (status, login_body) = call(
        &app,
        "POST",
        "/api/v1/auth/login",
        None,
        Some(json!({ "email": email, "password": password })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{login_body:?}");
    let old_refresh = login_body["refresh_token"].as_str().unwrap().to_string();

    let (status, refreshed) = call(
        &app,
        "POST",
        "/api/v1/auth/refresh",
        None,
        Some(json!({ "refresh_token": old_refresh })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{refreshed:?}");
    assert!(refreshed["access_token"].as_str().is_some());
    let new_refresh = refreshed["refresh_token"].as_str().unwrap().to_string();
    assert_ne!(old_refresh, new_refresh, "Refresh-Token muss rotieren");

    // Der alte Refresh-Token darf nach der Rotation nicht mehr funktionieren.
    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/auth/refresh",
        None,
        Some(json!({ "refresh_token": old_refresh })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Der neue funktioniert.
    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/auth/refresh",
        None,
        Some(json!({ "refresh_token": new_refresh })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn totp_enrollment_confirm_and_login_flow() {
    let Some((app, db)) = setup().await else {
        eprintln!("HAVENMAIL_TEST_DATABASE_URL nicht gesetzt — Test übersprungen");
        return;
    };
    let (email, password) = bootstrap_super_admin(&db).await;
    let access_token = login(&app, &email, &password).await;

    // Vor der Aktivierung: enabled=false, normaler Login ohne Code klappt.
    let (status, status_body) = call(
        &app,
        "GET",
        "/api/v1/users/me/totp",
        Some(&access_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{status_body:?}");
    assert_eq!(status_body["enabled"], json!(false));

    // Enrollment: Secret erzeugen, NICHT persistiert.
    let (status, enroll_body) = call(
        &app,
        "POST",
        "/api/v1/users/me/totp/enroll",
        Some(&access_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{enroll_body:?}");
    let secret = enroll_body["secret"].as_str().unwrap().to_string();
    assert!(enroll_body["otpauth_uri"]
        .as_str()
        .unwrap()
        .starts_with("otpauth://totp/"));

    // Confirm mit falschem Code schlägt fehl und aktiviert nichts.
    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/users/me/totp/confirm",
        Some(&access_token),
        Some(json!({ "secret": secret, "code": "000000" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Confirm mit echtem, aktuell gültigem Code aktiviert 2FA.
    let code = havenmail_core::auth::totp::generate_current_code(&secret).unwrap();
    let (status, confirm_body) = call(
        &app,
        "POST",
        "/api/v1/users/me/totp/confirm",
        Some(&access_token),
        Some(json!({ "secret": secret, "code": code })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{confirm_body:?}");

    let (_, status_body) = call(
        &app,
        "GET",
        "/api/v1/users/me/totp",
        Some(&access_token),
        None,
    )
    .await;
    assert_eq!(status_body["enabled"], json!(true));

    // Login ohne Code liefert jetzt totp_required statt Tokens.
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/auth/login",
        None,
        Some(json!({ "email": email, "password": password })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert_eq!(body["totp_required"], json!(true));
    assert!(body["access_token"].is_null());

    // Login mit falschem Code ebenfalls totp_required, keine Tokens.
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/auth/login",
        None,
        Some(json!({ "email": email, "password": password, "totp_code": "000000" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert_eq!(body["totp_required"], json!(true));

    // Login mit korrektem Code liefert echte Tokens.
    let code = havenmail_core::auth::totp::generate_current_code(&secret).unwrap();
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/auth/login",
        None,
        Some(json!({ "email": email, "password": password, "totp_code": code })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert!(body["access_token"].as_str().is_some());

    // Disable mit falschem Passwort schlägt fehl, 2FA bleibt aktiv.
    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/users/me/totp/disable",
        Some(&access_token),
        Some(json!({ "password": "definitiv-falsch" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Disable mit korrektem Passwort deaktiviert 2FA wieder.
    let (status, disable_body) = call(
        &app,
        "POST",
        "/api/v1/users/me/totp/disable",
        Some(&access_token),
        Some(json!({ "password": password })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{disable_body:?}");

    let (_, status_body) = call(
        &app,
        "GET",
        "/api/v1/users/me/totp",
        Some(&access_token),
        None,
    )
    .await;
    assert_eq!(status_body["enabled"], json!(false));

    // Normaler Login ohne Code funktioniert wieder.
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/auth/login",
        None,
        Some(json!({ "email": email, "password": password })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert!(body["access_token"].as_str().is_some());
}

#[tokio::test]
async fn session_list_marks_current_session_and_revoke_is_owner_scoped() {
    let Some((app, db)) = setup().await else {
        eprintln!("HAVENMAIL_TEST_DATABASE_URL nicht gesetzt — Test übersprungen");
        return;
    };
    let (email, password) = bootstrap_super_admin(&db).await;

    // Zwei "Geräte" -> zwei Sessions für denselben Nutzer.
    let (status, login_a) = call(
        &app,
        "POST",
        "/api/v1/auth/login",
        None,
        Some(json!({ "email": email, "password": password })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{login_a:?}");
    let token_a = login_a["access_token"].as_str().unwrap().to_string();

    let (status, login_b) = call(
        &app,
        "POST",
        "/api/v1/auth/login",
        None,
        Some(json!({ "email": email, "password": password })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{login_b:?}");

    // Mit Token A abgefragt: zwei Sessions, genau eine davon is_current.
    let (status, sessions) = call(
        &app,
        "GET",
        "/api/v1/users/me/sessions",
        Some(&token_a),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{sessions:?}");
    let sessions = sessions.as_array().expect("sessions ist ein Array");
    assert_eq!(sessions.len(), 2);
    let current_count = sessions
        .iter()
        .filter(|s| s["is_current"] == json!(true))
        .count();
    assert_eq!(current_count, 1, "{sessions:?}");
    let other_session_id = sessions
        .iter()
        .find(|s| s["is_current"] == json!(false))
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Ein anderer Nutzer darf die Session nicht sehen/widerrufen.
    let (other_email, other_password) = bootstrap_super_admin(&db).await;
    let other_token = login(&app, &other_email, &other_password).await;
    let (status, _) = call(
        &app,
        "DELETE",
        &format!("/api/v1/users/me/sessions/{other_session_id}"),
        Some(&other_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Der Eigentümer darf die andere eigene Session widerrufen.
    let (status, revoke_body) = call(
        &app,
        "DELETE",
        &format!("/api/v1/users/me/sessions/{other_session_id}"),
        Some(&token_a),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{revoke_body:?}");
    assert_eq!(revoke_body["was_current"], json!(false));

    // Danach nur noch eine (nicht widerrufene) Session sichtbar.
    let (_, sessions) = call(
        &app,
        "GET",
        "/api/v1/users/me/sessions",
        Some(&token_a),
        None,
    )
    .await;
    assert_eq!(sessions.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn api_token_authenticates_requests_and_can_be_revoked() {
    let Some((app, db)) = setup().await else {
        eprintln!("HAVENMAIL_TEST_DATABASE_URL nicht gesetzt — Test übersprungen");
        return;
    };
    let (email, password) = bootstrap_super_admin(&db).await;
    let access_token = login(&app, &email, &password).await;

    // Erzeugen.
    let (status, create_body) = call(
        &app,
        "POST",
        "/api/v1/users/me/api-tokens",
        Some(&access_token),
        Some(json!({ "scopes": ["ci-deploy"] })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{create_body:?}");
    let api_token = create_body["token"].as_str().unwrap().to_string();
    assert!(api_token.starts_with("hvm_"));
    let token_id = create_body["id"].as_str().unwrap().to_string();

    // Das API-Token selbst authentifiziert eine Anfrage — dieselben Rechte
    // wie der Account, der es erzeugt hat (super_admin hier).
    let (status, status_body) =
        call(&app, "GET", "/api/v1/system/status", Some(&api_token), None).await;
    assert_eq!(status, StatusCode::OK, "{status_body:?}");

    // Erscheint in der eigenen Liste (Klartext-Token nie wieder enthalten).
    let (status, list_body) = call(
        &app,
        "GET",
        "/api/v1/users/me/api-tokens",
        Some(&access_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{list_body:?}");
    let tokens = list_body.as_array().unwrap();
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0]["scopes"], json!(["ci-deploy"]));
    assert!(list_body.to_string().find(&api_token).is_none());

    // Fremder Nutzer darf es nicht widerrufen (404, kein Existenz-Hinweis).
    let (other_email, other_password) = bootstrap_super_admin(&db).await;
    let other_token = login(&app, &other_email, &other_password).await;
    let (status, _) = call(
        &app,
        "DELETE",
        &format!("/api/v1/users/me/api-tokens/{token_id}"),
        Some(&other_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Eigentümer widerruft es.
    let (status, _) = call(
        &app,
        "DELETE",
        &format!("/api/v1/users/me/api-tokens/{token_id}"),
        Some(&access_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Danach authentifiziert es keine Anfrage mehr.
    let (status, _) = call(&app, "GET", "/api/v1/system/status", Some(&api_token), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn csv_import_creates_valid_rows_and_reports_errors_for_bad_rows() {
    let Some((app, db)) = setup().await else {
        eprintln!("HAVENMAIL_TEST_DATABASE_URL nicht gesetzt — Test übersprungen");
        return;
    };
    let (super_email, super_password) = bootstrap_super_admin(&db).await;
    let super_token = login(&app, &super_email, &super_password).await;

    let domain_name = format!("csv-{}.test", Uuid::new_v4());
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/domains",
        Some(&super_token),
        Some(json!({ "name": domain_name })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    let domain_id = body["id"].as_str().unwrap().to_string();

    let csv = "local_part,password,role,quota_bytes\n\
               alice,alice-secret-pw123,user,\n\
               bob,bob-secret-pw123,domain_admin,104857600\n\
               tooshort,short,user,\n\
               alice,alice-secret-pw123,user,\n"; // Duplikat von Zeile 1

    let (status, import_body) = call(
        &app,
        "POST",
        &format!("/api/v1/domains/{domain_id}/users/import"),
        Some(&super_token),
        Some(json!({ "csv": csv })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{import_body:?}");
    let created = import_body["created"].as_array().unwrap();
    let errors = import_body["errors"].as_array().unwrap();
    assert_eq!(created.len(), 2, "{import_body:?}");
    assert_eq!(errors.len(), 2, "{import_body:?}");
    assert!(errors
        .iter()
        .any(|e| e["local_part"] == "tooshort" && e["message"].as_str().unwrap().contains("12")));
    assert!(errors
        .iter()
        .any(|e| e["local_part"] == "alice" && e["message"].as_str().unwrap().contains("bereits")));

    // Export enthält beide erfolgreich angelegten Zeilen, aber nie ein
    // Passwort/Passwort-Hash. Antwort ist rohes CSV, kein JSON — call()s
    // JSON-Parsing würde hier nur Value::Null liefern, also die Response
    // direkt lesen statt über call().
    let request = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/domains/{domain_id}/users/export"))
        .header("authorization", format!("Bearer {super_token}"))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let export_text = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(export_text.contains("alice"));
    assert!(export_text.contains("bob"));
    assert!(!export_text.contains("alice-secret-pw123"));
    assert!(!export_text.contains("bob-secret-pw123"));
}

#[tokio::test]
async fn domains_overview_aggregates_user_counts_and_is_super_admin_only() {
    let Some((app, db)) = setup().await else {
        eprintln!("HAVENMAIL_TEST_DATABASE_URL nicht gesetzt — Test übersprungen");
        return;
    };
    let (super_email, super_password) = bootstrap_super_admin(&db).await;
    let super_token = login(&app, &super_email, &super_password).await;

    let domain_name = format!("overview-{}.test", Uuid::new_v4());
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/domains",
        Some(&super_token),
        Some(json!({ "name": domain_name })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    let domain_id = body["id"].as_str().unwrap().to_string();

    for local_part in ["alice", "bob"] {
        let (status, _) = call(
            &app,
            "POST",
            &format!("/api/v1/domains/{domain_id}/users"),
            Some(&super_token),
            Some(json!({ "local_part": local_part, "password": "at-least-12-characters" })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    // /overview darf nicht als :domain_id-Route mit domain_id="overview"
    // fehlinterpretiert werden (Routenkonflikt-Regression).
    let (status, overview) = call(
        &app,
        "GET",
        "/api/v1/domains/overview",
        Some(&super_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{overview:?}");
    let entries = overview.as_array().expect("overview ist ein Array");
    let entry = entries
        .iter()
        .find(|e| e["id"] == domain_id)
        .expect("neue Domain fehlt in der Übersicht");
    assert_eq!(entry["user_count"], json!(2), "{entry:?}");
    assert_eq!(entry["is_active"], json!(true));

    // domain_admin darf die domänenübergreifende Übersicht nicht sehen.
    let admin_password = "domain-admin-secret-pw!!";
    let (status, _) = call(
        &app,
        "POST",
        &format!("/api/v1/domains/{domain_id}/users"),
        Some(&super_token),
        Some(json!({ "local_part": "admin", "password": admin_password, "role": "domain_admin" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let admin_token = login(&app, &format!("admin@{domain_name}"), admin_password).await;
    let (status, _) = call(
        &app,
        "GET",
        "/api/v1/domains/overview",
        Some(&admin_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn branding_get_is_public_and_patch_is_super_admin_only() {
    let Some((app, db)) = setup().await else {
        eprintln!("HAVENMAIL_TEST_DATABASE_URL nicht gesetzt — Test übersprungen");
        return;
    };
    let (super_email, super_password) = bootstrap_super_admin(&db).await;
    let super_token = login(&app, &super_email, &super_password).await;

    // GET ohne jedes Token (Login-Seite braucht das vor Authentifizierung).
    let (status, initial) = call(&app, "GET", "/api/v1/system/branding", None, None).await;
    assert_eq!(status, StatusCode::OK, "{initial:?}");
    assert_eq!(initial["product_name"], json!("Havenmail"));

    // Ungültige Logo-URL wird abgelehnt.
    let (status, _) = call(
        &app,
        "PATCH",
        "/api/v1/system/branding",
        Some(&super_token),
        Some(json!({ "product_name": "Acme Mail", "logo_url": "javascript:alert(1)" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Gültige Änderung durch super_admin.
    let (status, updated) = call(
        &app,
        "PATCH",
        "/api/v1/system/branding",
        Some(&super_token),
        Some(json!({
            "product_name": "Acme Mail",
            "logo_url": "https://example.org/logo.png",
            "accent_color": "#123456"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{updated:?}");
    assert_eq!(updated["product_name"], json!("Acme Mail"));
    assert_eq!(updated["logo_url"], json!("https://example.org/logo.png"));
    assert_eq!(updated["accent_color"], json!("#123456"));

    // GET (weiterhin ohne Token) zeigt die neuen Werte.
    let (_, after_patch) = call(&app, "GET", "/api/v1/system/branding", None, None).await;
    assert_eq!(after_patch["product_name"], json!("Acme Mail"));

    // domain_admin darf nicht ändern.
    let domain_name = format!("branding-{}.test", Uuid::new_v4());
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/domains",
        Some(&super_token),
        Some(json!({ "name": domain_name })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    let domain_id = body["id"].as_str().unwrap().to_string();
    let admin_password = "domain-admin-secret-pw!!";
    let (status, _) = call(
        &app,
        "POST",
        &format!("/api/v1/domains/{domain_id}/users"),
        Some(&super_token),
        Some(json!({ "local_part": "admin", "password": admin_password, "role": "domain_admin" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let admin_token = login(&app, &format!("admin@{domain_name}"), admin_password).await;
    let (status, _) = call(
        &app,
        "PATCH",
        "/api/v1/system/branding",
        Some(&admin_token),
        Some(json!({ "product_name": "Sollte nicht klappen" })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Aufräumen: Singleton-Zeile zurücksetzen, damit andere/künftige Tests
    // wieder von den Ist-Zustand-Defaults ausgehen können.
    let (status, _) = call(
        &app,
        "PATCH",
        "/api/v1/system/branding",
        Some(&super_token),
        Some(json!({ "product_name": "Havenmail" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn audit_log_supports_cursor_pagination_and_action_filter() {
    let Some((app, db)) = setup().await else {
        eprintln!("HAVENMAIL_TEST_DATABASE_URL nicht gesetzt — Test übersprungen");
        return;
    };
    let (super_email, super_password) = bootstrap_super_admin(&db).await;
    let super_token = login(&app, &super_email, &super_password).await;

    let domain_name = format!("audit-page-{}.test", Uuid::new_v4());
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/domains",
        Some(&super_token),
        Some(json!({ "name": domain_name })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    let domain_id = body["id"].as_str().unwrap().to_string();

    // Drei Aliase anlegen -> drei "alias.create"-Einträge in Reihenfolge.
    for source in ["a", "b", "c"] {
        let (status, _) = call(
            &app,
            "POST",
            &format!("/api/v1/domains/{domain_id}/aliases"),
            Some(&super_token),
            Some(json!({ "source": source, "destinations": ["ziel@example.org"] })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    // Erste Seite: limit=1 liefert genau den neuesten Eintrag ("c").
    let (status, page1) = call(
        &app,
        "GET",
        &format!("/api/v1/audit-log?domain_id={domain_id}&limit=1"),
        Some(&super_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{page1:?}");
    let page1 = page1.as_array().unwrap();
    assert_eq!(page1.len(), 1);
    assert_eq!(page1[0]["after"]["source"], json!("c"));
    let cursor = page1[0]["seq"].as_i64().unwrap();

    // Zweite Seite über before_seq: nächstälterer Eintrag ("b"), nicht "c" erneut.
    let (status, page2) = call(
        &app,
        "GET",
        &format!("/api/v1/audit-log?domain_id={domain_id}&limit=1&before_seq={cursor}"),
        Some(&super_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{page2:?}");
    let page2 = page2.as_array().unwrap();
    assert_eq!(page2.len(), 1);
    assert_eq!(page2[0]["after"]["source"], json!("b"));
    assert!(page2[0]["seq"].as_i64().unwrap() < cursor);

    // Aktionsfilter: nur "alias.create", alle drei ohne Limit-Beschränkung.
    let (status, filtered) = call(
        &app,
        "GET",
        &format!("/api/v1/audit-log?domain_id={domain_id}&action=alias.create"),
        Some(&super_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{filtered:?}");
    let filtered = filtered.as_array().unwrap();
    assert_eq!(filtered.len(), 3);
    assert!(filtered
        .iter()
        .all(|e| e["action"] == json!("alias.create")));

    // Aktionsliste enthält "alias.create".
    let (status, actions) = call(
        &app,
        "GET",
        "/api/v1/audit-log/actions",
        Some(&super_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{actions:?}");
    let actions = actions.as_array().unwrap();
    assert!(actions.iter().any(|a| a == "alias.create"));
}

#[tokio::test]
async fn cross_domain_search_scopes_by_role() {
    let Some((app, db)) = setup().await else {
        eprintln!("HAVENMAIL_TEST_DATABASE_URL nicht gesetzt — Test übersprungen");
        return;
    };
    let (super_email, super_password) = bootstrap_super_admin(&db).await;
    let super_token = login(&app, &super_email, &super_password).await;

    let unique = Uuid::new_v4().simple().to_string();
    let domain_a_name = format!("search-a-{unique}.test");
    let domain_b_name = format!("search-b-{unique}.test");

    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/domains",
        Some(&super_token),
        Some(json!({ "name": domain_a_name })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    let domain_a_id = body["id"].as_str().unwrap().to_string();

    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/domains",
        Some(&super_token),
        Some(json!({ "name": domain_b_name })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    let domain_b_id = body["id"].as_str().unwrap().to_string();

    let findme = format!("findme{unique}");
    let (status, _) = call(
        &app,
        "POST",
        &format!("/api/v1/domains/{domain_a_id}/users"),
        Some(&super_token),
        Some(json!({ "local_part": findme, "password": "at-least-12-characters" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // super_admin findet das Postfach über die Suche.
    let (status, results) = call(
        &app,
        "GET",
        &format!("/api/v1/search?q={findme}"),
        Some(&super_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{results:?}");
    let results = results.as_array().unwrap();
    assert!(results
        .iter()
        .any(|r| r["kind"] == "user" && r["local_part"] == json!(findme)));

    // Domain-Suche funktioniert ebenfalls.
    let (status, results) = call(
        &app,
        "GET",
        &format!("/api/v1/search?q=search-a-{unique}"),
        Some(&super_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{results:?}");
    let results = results.as_array().unwrap();
    assert!(results
        .iter()
        .any(|r| r["kind"] == "domain" && r["domain_name"] == json!(domain_a_name)));

    // domain_admin von Domain B findet das Postfach in Domain A NICHT.
    let admin_password = "domain-admin-secret-pw!!";
    let (status, _) = call(
        &app,
        "POST",
        &format!("/api/v1/domains/{domain_b_id}/users"),
        Some(&super_token),
        Some(json!({ "local_part": "admin", "password": admin_password, "role": "domain_admin" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let admin_token = login(&app, &format!("admin@{domain_b_name}"), admin_password).await;
    let (status, results) = call(
        &app,
        "GET",
        &format!("/api/v1/search?q={findme}"),
        Some(&admin_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{results:?}");
    assert_eq!(results.as_array().unwrap().len(), 0);

    // Zu kurze Anfrage liefert bewusst nichts, kein Full-Table-Scan-Ergebnis.
    let (status, results) = call(&app, "GET", "/api/v1/search?q=a", Some(&super_token), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(results.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn password_policy_is_readable_by_any_role_and_enforced_dynamically() {
    let Some((app, db)) = setup().await else {
        eprintln!("HAVENMAIL_TEST_DATABASE_URL nicht gesetzt — Test übersprungen");
        return;
    };
    let (super_email, super_password) = bootstrap_super_admin(&db).await;
    let super_token = login(&app, &super_email, &super_password).await;

    let domain_name = format!("pwpolicy-{}.test", Uuid::new_v4());
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/domains",
        Some(&super_token),
        Some(json!({ "name": domain_name })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    let domain_id = body["id"].as_str().unwrap().to_string();
    let admin_password = "pwpolicy-domain-admin-pw!!";
    let (status, _) = call(
        &app,
        "POST",
        &format!("/api/v1/domains/{domain_id}/users"),
        Some(&super_token),
        Some(json!({ "local_part": "admin", "password": admin_password, "role": "domain_admin" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let admin_token = login(&app, &format!("admin@{domain_name}"), admin_password).await;

    // GET ist für jede eingeloggte Rolle erreichbar (kein ManageSystemSettings-
    // Gate) — ein domain_admin braucht die Mindestlänge, um sie im eigenen
    // Postfach-Anlegen-Formular anzuzeigen.
    let (status, policy) = call(
        &app,
        "GET",
        "/api/v1/system/password-policy",
        Some(&admin_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{policy:?}");
    assert!(policy["min_password_length"].as_i64().is_some());

    // domain_admin darf die Richtlinie nicht ändern.
    let (status, _) = call(
        &app,
        "PATCH",
        "/api/v1/system/password-policy",
        Some(&admin_token),
        Some(json!({ "min_password_length": 20 })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Werte unter 8 werden abgelehnt (hartes Minimum, siehe Migration 0010).
    let (status, _) = call(
        &app,
        "PATCH",
        "/api/v1/system/password-policy",
        Some(&super_token),
        Some(json!({ "min_password_length": 7 })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // super_admin hebt die Mindestlänge auf 15 an. Bewusst nur +3 gegenüber
    // dem Default 12 und weit unter den kürzesten in anderen Tests dieser
    // Datei verwendeten Passwörtern (>= 16 Zeichen) — Tests laufen parallel
    // gegen dieselbe Singleton-Zeile, ein zu aggressiver Wert würde
    // gleichzeitig laufende Postfach-Anlagen anderer Tests brechen.
    let (status, updated) = call(
        &app,
        "PATCH",
        "/api/v1/system/password-policy",
        Some(&super_token),
        Some(json!({ "min_password_length": 15 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{updated:?}");
    assert_eq!(updated["min_password_length"], json!(15));

    // Ein 12-Zeichen-Passwort (nach dem alten Default gültig) wird jetzt abgelehnt...
    let (status, body) = call(
        &app,
        "POST",
        &format!("/api/v1/domains/{domain_id}/users"),
        Some(&admin_token),
        Some(json!({ "local_part": "toosecret", "password": "short1234567" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body:?}");

    // ...ein 15-Zeichen-Passwort wird weiterhin akzeptiert.
    let (status, body) = call(
        &app,
        "POST",
        &format!("/api/v1/domains/{domain_id}/users"),
        Some(&admin_token),
        Some(json!({ "local_part": "longenough", "password": "exactly15chars!" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");

    // Aufräumen: Singleton-Zeile zurücksetzen, damit andere/künftige Tests
    // wieder vom Migrations-Default ausgehen können (gleiches Muster wie bei
    // branding_get_is_public_and_patch_is_super_admin_only).
    let (status, _) = call(
        &app,
        "PATCH",
        "/api/v1/system/password-policy",
        Some(&super_token),
        Some(json!({ "min_password_length": 12 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

/// Testet nur die Vorprüfungen (RBAC, Validierung), die VOR
/// `security_settings::apply_to_rspamd` laufen — der Erfolgspfad selbst
/// schreibt echte Dateien nach `/etc/rspamd/local.d/` und ruft `rspamadm
/// configtest` auf, was in der CI-Umgebung (kein Rspamd installiert,
/// siehe .github/workflows/backend.yml) fehlschlagen würde. Der
/// Erfolgspfad wurde stattdessen live gegen den echten Produktionsserver
/// verifiziert (siehe Commit-Beschreibung).
#[tokio::test]
async fn ratelimit_override_is_scoped_and_validated() {
    let Some((app, db)) = setup().await else {
        eprintln!("HAVENMAIL_TEST_DATABASE_URL nicht gesetzt — Test übersprungen");
        return;
    };
    let (super_email, super_password) = bootstrap_super_admin(&db).await;
    let super_token = login(&app, &super_email, &super_password).await;

    let domain_a_name = format!("ratelimit-a-{}.test", Uuid::new_v4());
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/domains",
        Some(&super_token),
        Some(json!({ "name": domain_a_name })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    let domain_a_id = body["id"].as_str().unwrap().to_string();

    let domain_b_name = format!("ratelimit-b-{}.test", Uuid::new_v4());
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/domains",
        Some(&super_token),
        Some(json!({ "name": domain_b_name })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    let domain_b_id = body["id"].as_str().unwrap().to_string();

    let admin_password = "ratelimit-domain-admin-pw!!";
    let (status, _) = call(
        &app,
        "POST",
        &format!("/api/v1/domains/{domain_b_id}/users"),
        Some(&super_token),
        Some(json!({ "local_part": "admin", "password": admin_password, "role": "domain_admin" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let admin_b_token = login(&app, &format!("admin@{domain_b_name}"), admin_password).await;

    // domain_admin von Domain B darf Domain A's Override nicht anfassen
    // (gleiche NotFound-statt-Forbidden-Konvention wie update_domain, um
    // die Existenz fremder Domains nicht preiszugeben).
    let (status, _) = call(
        &app,
        "PATCH",
        &format!("/api/v1/domains/{domain_a_id}/ratelimit-override"),
        Some(&admin_b_token),
        Some(json!({ "ratelimit_per_hour_override": 50, "ratelimit_burst_override": 50 })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Werte unter 1 werden abgelehnt — Prüfung läuft vor jedem
    // Rspamd-Zugriff, daher CI-sicher.
    let (status, body) = call(
        &app,
        "PATCH",
        &format!("/api/v1/domains/{domain_a_id}/ratelimit-override"),
        Some(&super_token),
        Some(json!({ "ratelimit_per_hour_override": 0, "ratelimit_burst_override": null })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body:?}");

    // Neu angelegte Domain hat standardmäßig keinen Override gesetzt.
    let (status, body) = call(
        &app,
        "GET",
        &format!("/api/v1/domains/{domain_a_id}"),
        Some(&super_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert_eq!(body["ratelimit_per_hour_override"], Value::Null);
    assert_eq!(body["ratelimit_burst_override"], Value::Null);
}
