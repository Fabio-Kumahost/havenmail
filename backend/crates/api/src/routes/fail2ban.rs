//! Fail2Ban-Übersicht/Entsperren (Admin-Panel) — nur `super_admin`
//! (`Action::ManageSystemSettings`, wie `system.rs`).
//!
//! Die API selbst spricht nie mit fail2ban direkt (dessen Steuer-Socket
//! ist root:root 0600) — sie liest nur die Statusdatei, die ein root-
//! eigener systemd-Timer alle 30s schreibt, und stößt Entsperrungen über
//! dasselbe Trigger-Datei-Muster wie den Rspamd-Reload und die Mail-
//! Warteschlangen-Löschung an (siehe routes/security_settings.rs,
//! routes/mail_queue.rs).

use crate::auth_extractor::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::{extract::State, http::HeaderMap, Json};
use havenmail_core::fail2ban::{is_valid_jail, Fail2banStatus};
use havenmail_core::rbac::Action;
use serde::Deserialize;

fn state_dir() -> String {
    std::env::var("HAVENMAIL_STATE_DIR").unwrap_or_else(|_| "/var/lib/havenmail".to_string())
}

pub async fn get_status(AuthUser(actor): AuthUser) -> ApiResult<Json<Fail2banStatus>> {
    if !actor.can(Action::ManageSystemSettings, None) {
        return Err(ApiError::Forbidden);
    }
    let path = format!("{}/fail2ban-status.json", state_dir());
    let content = std::fs::read_to_string(&path).map_err(|e| {
        ApiError::BadRequest(format!(
            "Fail2Ban-Status noch nicht verfügbar (Timer lief evtl. noch nicht): {e}"
        ))
    })?;
    let status: Fail2banStatus = serde_json::from_str(&content)
        .map_err(|e| ApiError::BadRequest(format!("Status-Datei beschädigt: {e}")))?;
    Ok(Json(status))
}

#[derive(Debug, Deserialize)]
pub struct UnbanRequest {
    pub jail: String,
    pub ip: String,
}

pub async fn unban(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    headers: HeaderMap,
    Json(req): Json<UnbanRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    if !actor.can(Action::ManageSystemSettings, None) {
        return Err(ApiError::Forbidden);
    }
    if !is_valid_jail(&req.jail) {
        return Err(ApiError::BadRequest("unbekanntes Jail".to_string()));
    }
    if req.ip.parse::<std::net::IpAddr>().is_err() {
        return Err(ApiError::BadRequest("ungültige IP-Adresse".to_string()));
    }

    let trigger_path = std::path::PathBuf::from(format!("{}/fail2ban-unban-request", state_dir()));
    havenmail_core::trigger_file::write(&trigger_path, &format!("{}:{}", req.jail, req.ip))
        .map_err(|e| ApiError::BadRequest(format!("Entsperr-Anfrage fehlgeschlagen: {e}")))?;

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    crate::audit_log::log(
        &state,
        &actor,
        &headers,
        "fail2ban.unban",
        &format!("{}:{}", req.jail, req.ip),
        None,
        None,
        None,
    )
    .await;

    Ok(Json(serde_json::json!({ "status": "requested" })))
}
