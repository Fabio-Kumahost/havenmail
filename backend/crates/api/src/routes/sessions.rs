//! Selbstbedienungs-Sitzungsverwaltung — welche Refresh-Sessions (Geräte/
//! Browser) für das eigene Konto aktuell gültig sind, mit der Möglichkeit,
//! einzelne aus der Ferne abzumelden. Nutzt die schon bestehende
//! `sessions`-Tabelle (samt `ip`/`user_agent`, seit `routes/auth.rs` diese
//! beim Login/Refresh mitschreibt) — kein neues Datenmodell nötig.

use crate::auth_extractor::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    Json,
};
use havenmail_core::rbac::Action;
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, FromRow)]
struct SessionRow {
    id: Uuid,
    ip: Option<String>,
    user_agent: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct SessionEntry {
    pub id: Uuid,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// `true` für genau die Sitzung, deren Refresh-Token das gerade
    /// verwendete Access-Token ausgestellt hat (siehe
    /// `auth::jwt::Claims::session_id`) — das Frontend warnt beim
    /// Abmelden dieser Sitzung, da es die eigene ist.
    pub is_current: bool,
}

/// Nur nicht widerrufene Sitzungen — abgelaufene/widerrufene sind für die
/// Selbstbedienungsansicht uninteressant (kein Audit-Log-Ersatz, dafür
/// gibt es `/audit-log`).
pub async fn list_sessions(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
) -> ApiResult<Json<Vec<SessionEntry>>> {
    if !actor.can(Action::ManageOwnAccount, None) {
        return Err(ApiError::Forbidden);
    }

    let rows: Vec<SessionRow> = sqlx::query_as(
        r#"
        SELECT id, host(ip) as ip, user_agent, created_at
        FROM sessions
        WHERE user_id = $1 AND revoked_at IS NULL
        ORDER BY created_at DESC
        "#,
    )
    .bind(actor.user_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(
        rows.into_iter()
            .map(|r| SessionEntry {
                is_current: r.id == actor.session_id,
                id: r.id,
                ip: r.ip,
                user_agent: r.user_agent,
                created_at: r.created_at,
            })
            .collect(),
    ))
}

pub async fn revoke_session(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(session_id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    if !actor.can(Action::ManageOwnAccount, None) {
        return Err(ApiError::Forbidden);
    }

    let owner: Option<Uuid> = sqlx::query_scalar("SELECT user_id FROM sessions WHERE id = $1")
        .bind(session_id)
        .fetch_optional(&state.db)
        .await?;

    match owner {
        Some(user_id) if actor.owns(user_id) => {}
        // Fremde oder nicht existente Sitzung -> wie "nicht gefunden"
        // behandeln, kein Hinweis auf Existenz der Session eines anderen
        // Nutzers (gleiches Muster wie logout() in routes/auth.rs).
        _ => return Err(ApiError::NotFound),
    }

    sqlx::query("UPDATE sessions SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL")
        .bind(session_id)
        .execute(&state.db)
        .await?;

    Ok(Json(serde_json::json!({
        "status": "revoked",
        "was_current": session_id == actor.session_id,
    })))
}
