//! Selbstbedienungs-API-Keys für Automatisierung (CI-Skripte, externe
//! Integrationen) — getrennt von der interaktiven JWT-/Session-Anmeldung,
//! damit ein Skript kein Passwort und keinen 2FA-Code braucht und einzeln
//! widerrufen werden kann, ohne die eigene Browser-Sitzung zu betreffen.
//! Nutzt die `api_tokens`-Tabelle, die seit dem allerersten Schema bereitlag,
//! aber nie angeschlossen war, und denselben Opak-Token-Mechanismus wie
//! Refresh-Tokens (`core::auth::token`, SHA-256-gehasht, `hvm_`-Präfix —
//! `auth_extractor.rs` erkennt daran ein API-Token statt eines JWTs).
//!
//! `scopes` sind aktuell freie Text-Labels zur eigenen Orientierung (z. B.
//! "ci-deploy", "monitoring") — ein API-Token hat exakt dieselben Rechte
//! wie der Account, der es erzeugt hat (Rolle/Domain-Scope werden bei jeder
//! Anfrage frisch aus `users` gelesen, siehe `auth_extractor.rs`), keine
//! feingranularere Restriktion je Scope-String. Das Schema erlaubt das für
//! später, ohne dass hier vorgegriffen werden soll.

use crate::auth_extractor::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    Json,
};
use havenmail_core::auth::token;
use havenmail_core::rbac::Action;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, FromRow, Serialize)]
pub struct ApiTokenEntry {
    pub id: Uuid,
    pub scopes: Vec<String>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn list_api_tokens(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
) -> ApiResult<Json<Vec<ApiTokenEntry>>> {
    if !actor.can(Action::ManageOwnAccount, None) {
        return Err(ApiError::Forbidden);
    }

    let rows: Vec<ApiTokenEntry> = sqlx::query_as(
        r#"
        SELECT id, scopes, expires_at, created_at
        FROM api_tokens
        WHERE user_id = $1 AND revoked_at IS NULL
        ORDER BY created_at DESC
        "#,
    )
    .bind(actor.user_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct CreateApiTokenRequest {
    #[serde(default)]
    pub scopes: Vec<String>,
    /// `None` = läuft nie ab (z. B. für dauerhafte CI-Zugänge).
    #[serde(default)]
    pub expires_in_days: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct CreateApiTokenResponse {
    pub id: Uuid,
    /// Klartext-Token — wird NUR hier einmalig ausgegeben, danach nur noch
    /// der Hash gespeichert (gleiches Prinzip wie Refresh-Tokens).
    pub token: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn create_api_token(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    headers: axum::http::HeaderMap,
    Json(req): Json<CreateApiTokenRequest>,
) -> ApiResult<Json<CreateApiTokenResponse>> {
    if !actor.can(Action::ManageOwnAccount, None) {
        return Err(ApiError::Forbidden);
    }
    if let Some(days) = req.expires_in_days {
        if days < 1 {
            return Err(ApiError::BadRequest(
                "expires_in_days muss mindestens 1 sein, falls angegeben".to_string(),
            ));
        }
    }

    let (plaintext, hash) = token::generate_opaque_token();
    let expires_at = req
        .expires_in_days
        .map(|days| chrono::Utc::now() + chrono::Duration::days(days));

    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO api_tokens (user_id, scopes, token_hash, expires_at)
        VALUES ($1, $2, $3, $4)
        RETURNING id
        "#,
    )
    .bind(actor.user_id)
    .bind(&req.scopes)
    .bind(&hash)
    .bind(expires_at)
    .fetch_one(&state.db)
    .await?;

    crate::audit_log::log(
        &state,
        &actor,
        &headers,
        "api_token.create",
        &id.to_string(),
        None,
        None,
        Some(serde_json::json!({ "scopes": req.scopes })),
    )
    .await;

    Ok(Json(CreateApiTokenResponse {
        id,
        token: plaintext,
        scopes: req.scopes,
        expires_at,
    }))
}

pub async fn revoke_api_token(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(token_id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    if !actor.can(Action::ManageOwnAccount, None) {
        return Err(ApiError::Forbidden);
    }

    let owner: Option<Uuid> = sqlx::query_scalar("SELECT user_id FROM api_tokens WHERE id = $1")
        .bind(token_id)
        .fetch_optional(&state.db)
        .await?;

    match owner {
        Some(user_id) if actor.owns(user_id) => {}
        _ => return Err(ApiError::NotFound),
    }

    sqlx::query("UPDATE api_tokens SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL")
        .bind(token_id)
        .execute(&state.db)
        .await?;

    Ok(Json(serde_json::json!({ "status": "revoked" })))
}
