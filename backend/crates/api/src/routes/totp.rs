//! Selbstbedienungs-TOTP-Zwei-Faktor-Authentifizierung — analog zu
//! `change_own_password` in `users.rs`: über `AuthUser` aus dem JWT
//! aufgelöst, kein `:user_id`-Pfadparameter (nur das eigene Konto).
//!
//! Enrollment ist zweistufig, damit ein abgebrochener Vorgang niemanden
//! aussperrt: `enroll` erzeugt Secret + `otpauth://`-URI und persistiert
//! NICHTS. Erst `confirm` — mit demselben Secret und einem damit gültigen
//! Code als Beweis, dass der Nutzer es erfolgreich in seine Authenticator-
//! App gescannt hat — verschlüsselt und speichert es in
//! `users.totp_secret_enc`. `disable` verlangt zusätzlich das aktuelle
//! Passwort (Schutz gegen eine gekaperte Sitzung, die sonst einfach 2FA
//! abschalten könnte).

use crate::auth_extractor::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::{extract::State, http::HeaderMap, Json};
use havenmail_core::auth::{password, totp};
use havenmail_core::rbac::Action;
use havenmail_core::secrets_crypto;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, FromRow)]
struct TotpRow {
    totp_secret_enc: Option<Vec<u8>>,
    local_part: String,
    domain_name: String,
}

async fn fetch_totp_row(state: &AppState, user_id: uuid::Uuid) -> ApiResult<TotpRow> {
    sqlx::query_as(
        r#"
        SELECT u.totp_secret_enc, u.local_part, d.name as domain_name
        FROM users u JOIN domains d ON d.id = u.domain_id
        WHERE u.id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound)
}

#[derive(Debug, Serialize)]
pub struct TotpStatus {
    pub enabled: bool,
}

pub async fn get_status(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
) -> ApiResult<Json<TotpStatus>> {
    if !actor.can(Action::ManageOwnAccount, None) {
        return Err(ApiError::Forbidden);
    }
    let row = fetch_totp_row(&state, actor.user_id).await?;
    Ok(Json(TotpStatus {
        enabled: row.totp_secret_enc.is_some(),
    }))
}

#[derive(Debug, Serialize)]
pub struct EnrollResponse {
    pub secret: String,
    pub otpauth_uri: String,
}

/// Erzeugt ein neues Secret, speichert NICHTS — siehe Modul-Dokumentation.
pub async fn enroll(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
) -> ApiResult<Json<EnrollResponse>> {
    if !actor.can(Action::ManageOwnAccount, None) {
        return Err(ApiError::Forbidden);
    }
    let row = fetch_totp_row(&state, actor.user_id).await?;
    let account_email = format!("{}@{}", row.local_part, row.domain_name);

    let (secret, otpauth_uri) = totp::generate_secret(&account_email, "Havenmail")
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    Ok(Json(EnrollResponse {
        secret,
        otpauth_uri,
    }))
}

#[derive(Debug, Deserialize)]
pub struct ConfirmRequest {
    pub secret: String,
    pub code: String,
}

pub async fn confirm(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    headers: HeaderMap,
    Json(req): Json<ConfirmRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    if !actor.can(Action::ManageOwnAccount, None) {
        return Err(ApiError::Forbidden);
    }

    let valid = totp::verify_code(&req.secret, &req.code)
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    if !valid {
        return Err(ApiError::BadRequest(
            "Code ist ungültig oder abgelaufen".to_string(),
        ));
    }

    let encrypted = secrets_crypto::encrypt(&state.secrets_key, &req.secret)
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    sqlx::query("UPDATE users SET totp_secret_enc = $2 WHERE id = $1")
        .bind(actor.user_id)
        .bind(&encrypted)
        .execute(&state.db)
        .await?;

    crate::audit_log::log(
        &state,
        &actor,
        &headers,
        "user.enable_totp",
        &actor.user_id.to_string(),
        None,
        None,
        None,
    )
    .await;

    Ok(Json(serde_json::json!({ "status": "enabled" })))
}

#[derive(Debug, Deserialize)]
pub struct DisableRequest {
    pub password: String,
}

pub async fn disable(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    headers: HeaderMap,
    Json(req): Json<DisableRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    if !actor.can(Action::ManageOwnAccount, None) {
        return Err(ApiError::Forbidden);
    }

    #[derive(Debug, FromRow)]
    struct PasswordHashRow {
        password_hash: String,
    }
    let row: PasswordHashRow = sqlx::query_as("SELECT password_hash FROM users WHERE id = $1")
        .bind(actor.user_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;

    if !password::verify_password(&req.password, &row.password_hash) {
        return Err(ApiError::BadRequest("Passwort ist falsch".to_string()));
    }

    sqlx::query("UPDATE users SET totp_secret_enc = NULL WHERE id = $1")
        .bind(actor.user_id)
        .execute(&state.db)
        .await?;

    crate::audit_log::log(
        &state,
        &actor,
        &headers,
        "user.disable_totp",
        &actor.user_id.to_string(),
        None,
        None,
        None,
    )
    .await;

    Ok(Json(serde_json::json!({ "status": "disabled" })))
}
