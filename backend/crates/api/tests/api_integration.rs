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
