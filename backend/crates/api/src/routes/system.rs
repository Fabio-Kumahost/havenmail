//! System-/Dienststatus für die Admin-Oberfläche — nur `super_admin`
//! (`Action::ManageSystemSettings`). Zeigt, ob die orchestrierten
//! Mail-Engines tatsächlich laufen, nicht nur ob die Control-Plane-API
//! selbst erreichbar ist (das prüft bereits `/healthz`/`/readyz`).

use crate::auth_extractor::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::{extract::State, Json};
use havenmail_core::rbac::Action;
use serde::Serialize;

/// Dienste, die der Installer orchestriert (siehe
/// scripts/lib/install_steps.sh, havenmail_start_services). `nginx` und
/// `havenmail-api` bewusst mit aufgeführt — ein Reload/Neustart, der die
/// eigene Erreichbarkeit nicht beeinträchtigt hat, soll trotzdem sichtbar
/// sein.
const MANAGED_UNITS: &[&str] = &[
    "havenmail-api",
    "postfix",
    "dovecot",
    "rspamd",
    "clamav-daemon",
    "nginx",
    "fail2ban",
];

#[derive(Debug, Serialize)]
pub struct ServiceStatus {
    pub unit: String,
    /// `true`, wenn `systemctl is-active` "active" meldet. `false` deckt
    /// sowohl "inactive"/"failed" als auch den Fall ab, dass systemctl
    /// selbst nicht aufgerufen werden konnte (z. B. lokale Entwicklung
    /// ohne systemd) — dort steht `detail` auf "unknown".
    pub active: bool,
    pub detail: String,
}

#[derive(Debug, Serialize)]
pub struct SystemStatusResponse {
    pub database: bool,
    pub services: Vec<ServiceStatus>,
}

async fn query_unit_status(unit: &str) -> ServiceStatus {
    match tokio::process::Command::new("systemctl")
        .args(["is-active", unit])
        .output()
        .await
    {
        Ok(output) => {
            let detail = String::from_utf8_lossy(&output.stdout).trim().to_string();
            ServiceStatus {
                unit: unit.to_string(),
                active: detail == "active",
                detail,
            }
        }
        Err(_) => ServiceStatus {
            unit: unit.to_string(),
            active: false,
            detail: "unknown".to_string(),
        },
    }
}

pub async fn system_status(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
) -> ApiResult<Json<SystemStatusResponse>> {
    if !actor.can(Action::ManageSystemSettings, None) {
        return Err(ApiError::Forbidden);
    }

    let database = havenmail_core::db::check_connectivity(&state.db).await;

    let mut services = Vec::with_capacity(MANAGED_UNITS.len());
    for unit in MANAGED_UNITS {
        services.push(query_unit_status(unit).await);
    }

    Ok(Json(SystemStatusResponse { database, services }))
}
