//! Backup auslösen/Verlauf (Admin-Panel, System-Seite) — nur
//! `super_admin` (`Action::ManageSystemSettings`).
//!
//! Die API selbst führt `backup.sh` nie aus und liest nie ein Archiv —
//! das Skript braucht root (u. a. Lesezugriff auf /etc/havenmail inkl.
//! HAVENMAIL_SECRETS_KEY). Auslösen läuft über dasselbe Trigger-Datei-
//! Muster wie Rspamd-Reload/Mail-Queue-Löschung/Fail2Ban-Entsperrung;
//! gelesen wird nur die vom root-eigenen Dienst geschriebene Status-JSON.

use crate::auth_extractor::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::{extract::State, http::HeaderMap, Json};
use havenmail_core::rbac::Action;

fn state_dir() -> String {
    std::env::var("HAVENMAIL_STATE_DIR").unwrap_or_else(|_| "/var/lib/havenmail".to_string())
}

pub async fn get_status(AuthUser(actor): AuthUser) -> ApiResult<Json<serde_json::Value>> {
    if !actor.can(Action::ManageSystemSettings, None) {
        return Err(ApiError::Forbidden);
    }
    let path = format!("{}/backup-status.json", state_dir());
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            let value: serde_json::Value = serde_json::from_str(&content)
                .map_err(|e| ApiError::BadRequest(format!("Status-Datei beschädigt: {e}")))?;
            Ok(Json(value))
        }
        // Noch nie ein Backup gelaufen — kein Fehler, sondern ein
        // legitimer Anfangszustand (z. B. direkt nach der Installation).
        Err(_) => Ok(Json(serde_json::json!({ "last_run": null, "history": [] }))),
    }
}

pub async fn trigger(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    headers: HeaderMap,
) -> ApiResult<Json<serde_json::Value>> {
    if !actor.can(Action::ManageSystemSettings, None) {
        return Err(ApiError::Forbidden);
    }

    let trigger_path = std::path::PathBuf::from(format!("{}/backup-trigger-request", state_dir()));
    havenmail_core::trigger_file::write(&trigger_path, &chrono::Utc::now().to_rfc3339())
        .map_err(|e| ApiError::BadRequest(format!("Backup-Anfrage fehlgeschlagen: {e}")))?;

    crate::audit_log::log(
        &state,
        &actor,
        &headers,
        "backup.trigger",
        "backup",
        None,
        None,
        None,
    )
    .await;

    // Kein synchrones Warten wie beim Rspamd-Reload/Queue-Löschen: ein
    // echtes Backup (pg_dump + tar über potenziell große Maildaten)
    // kann Minuten dauern, das würde die HTTP-Anfrage viel zu lange
    // offen halten. Das Frontend pollt stattdessen GET /system/backup.
    Ok(Json(serde_json::json!({ "status": "requested" })))
}
