//! Mail-Warteschlange (Admin-Panel "Warteschlange leeren") — nur
//! `super_admin` (`Action::ManageSystemSettings`, wie `system.rs`).
//!
//! Löschen läuft nicht direkt: `postsuper -d` ist laut Postfix selbst
//! "reserved for the superuser" und der `havenmail`-Systembenutzer hat
//! bewusst kein root — genau wie beim Rspamd-Reload (siehe
//! routes/security_settings.rs) schreibt dieser Handler nur eine
//! validierte Trigger-Datei, ein separates root-eigenes systemd-Path-Unit
//! (havenmail-queue-delete.{service,path}) führt die eigentliche Löschung
//! aus. Da Path-Units asynchron auslösen, wartet der Handler kurz und
//! verifiziert das Ergebnis über eine erneute (unprivilegierte)
//! `postqueue`-Abfrage, statt dem Trigger blind zu vertrauen.

use crate::auth_extractor::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use havenmail_core::mail_queue::{list_queue, request_delete, QueueEntry};
use havenmail_core::rbac::Action;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct QueueEntryResponse {
    pub queue_id: String,
    pub queue_name: String,
    pub arrival_time: chrono::DateTime<chrono::Utc>,
    pub message_size: i64,
    pub sender: String,
    pub recipients: Vec<havenmail_core::mail_queue::QueueRecipient>,
}

impl From<QueueEntry> for QueueEntryResponse {
    fn from(e: QueueEntry) -> Self {
        Self {
            queue_id: e.queue_id,
            queue_name: e.queue_name,
            arrival_time: chrono::DateTime::from_timestamp(e.arrival_time, 0)
                .unwrap_or_else(chrono::Utc::now),
            message_size: e.message_size,
            sender: e.sender,
            recipients: e.recipients,
        }
    }
}

fn state_dir() -> String {
    std::env::var("HAVENMAIL_STATE_DIR").unwrap_or_else(|_| "/var/lib/havenmail".to_string())
}

pub async fn list_mail_queue(
    AuthUser(actor): AuthUser,
) -> ApiResult<Json<Vec<QueueEntryResponse>>> {
    if !actor.can(Action::ManageSystemSettings, None) {
        return Err(ApiError::Forbidden);
    }
    let entries = list_queue()
        .await
        .map_err(|e| ApiError::BadRequest(format!("Warteschlange nicht lesbar: {e}")))?;
    Ok(Json(entries.into_iter().map(Into::into).collect()))
}

pub async fn delete_queue_entry(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(queue_id): Path<String>,
    headers: HeaderMap,
) -> ApiResult<Json<serde_json::Value>> {
    if !actor.can(Action::ManageSystemSettings, None) {
        return Err(ApiError::Forbidden);
    }

    let before = list_queue()
        .await
        .map_err(|e| ApiError::BadRequest(format!("Warteschlange nicht lesbar: {e}")))?;
    if !before.iter().any(|e| e.queue_id == queue_id) {
        return Err(ApiError::NotFound);
    }

    request_delete(std::path::Path::new(&state_dir()), &queue_id)
        .map_err(|e| ApiError::BadRequest(format!("Löschanfrage fehlgeschlagen: {e}")))?;

    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    let after = list_queue().await.unwrap_or_default();
    let removed = !after.iter().any(|e| e.queue_id == queue_id);

    crate::audit_log::log(
        &state,
        &actor,
        &headers,
        "mail_queue.delete_entry",
        &queue_id,
        None,
        None,
        None,
    )
    .await;

    Ok(Json(
        serde_json::json!({ "status": if removed { "deleted" } else { "pending" } }),
    ))
}

pub async fn delete_all_queue(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    headers: HeaderMap,
) -> ApiResult<Json<serde_json::Value>> {
    if !actor.can(Action::ManageSystemSettings, None) {
        return Err(ApiError::Forbidden);
    }

    let before_count = list_queue().await.map(|q| q.len()).unwrap_or(0);
    if before_count == 0 {
        return Ok(Json(serde_json::json!({ "status": "already_empty" })));
    }

    request_delete(std::path::Path::new(&state_dir()), "ALL")
        .map_err(|e| ApiError::BadRequest(format!("Löschanfrage fehlgeschlagen: {e}")))?;

    tokio::time::sleep(std::time::Duration::from_millis(600)).await;
    let after_count = list_queue().await.map(|q| q.len()).unwrap_or(before_count);

    crate::audit_log::log(
        &state,
        &actor,
        &headers,
        "mail_queue.delete_all",
        "ALL",
        None,
        Some(serde_json::json!({ "queue_size": before_count })),
        Some(serde_json::json!({ "queue_size": after_count })),
    )
    .await;

    Ok(Json(serde_json::json!({
        "status": if after_count == 0 { "deleted" } else { "pending" },
        "removed": before_count.saturating_sub(after_count),
        "remaining": after_count,
    })))
}
