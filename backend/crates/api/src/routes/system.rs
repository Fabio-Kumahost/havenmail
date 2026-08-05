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
pub struct TlsStatus {
    /// Rohes Ablaufdatum, wie `openssl x509 -enddate` es ausgibt (z. B.
    /// "Nov  3 12:00:00 2026 GMT").
    pub expires_at: String,
    /// `None`, wenn das Datum nicht geparst werden konnte — `expires_at`
    /// wird trotzdem angezeigt, nur ohne Tage-Countdown.
    pub days_remaining: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct SystemStatusResponse {
    pub database: bool,
    pub services: Vec<ServiceStatus>,
    /// `None`, solange kein Zertifikat ausgestellt wurde (z. B. lokale
    /// Entwicklung ohne install.sh-Lauf) — siehe
    /// scripts/lib/install_steps.sh, havenmail_write_tls_expiry_file.
    pub tls: Option<TlsStatus>,
}

/// Liest NUR das Ablaufdatum, das install.sh/der certbot-Deploy-Hook nach
/// `${HAVENMAIL_ETC_DIR}/tls-expiry` schreibt (0644) — die API bekommt so
/// Sichtbarkeit auf die Zertifikatslaufzeit, ohne selbst Lesezugriff auf
/// `/etc/letsencrypt` (root:root 0700, enthält den privaten Schlüssel) zu
/// benötigen.
/// Parst das openssl-`-enddate`-Format ("Nov  3 12:00:00 2026 GMT", immer
/// GMT/UTC) zu Tagen bis zum Ablauf ab `now`. Eigene Funktion statt inline
/// in `read_tls_status`, damit sie ohne Dateisystem/Env testbar ist.
fn days_remaining(raw_expiry: &str, now: chrono::DateTime<chrono::Utc>) -> Option<i64> {
    let without_tz = raw_expiry.trim().trim_end_matches("GMT").trim();
    let naive = chrono::NaiveDateTime::parse_from_str(without_tz, "%b %e %H:%M:%S %Y").ok()?;
    Some((naive.and_utc() - now).num_days())
}

fn read_tls_status() -> Option<TlsStatus> {
    let etc_dir =
        std::env::var("HAVENMAIL_ETC_DIR").unwrap_or_else(|_| "/etc/havenmail".to_string());
    let raw = std::fs::read_to_string(format!("{etc_dir}/tls-expiry")).ok()?;
    let expires_at = raw.trim().to_string();
    if expires_at.is_empty() {
        return None;
    }

    Some(TlsStatus {
        days_remaining: days_remaining(&expires_at, chrono::Utc::now()),
        expires_at,
    })
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

    let tls = read_tls_status();

    Ok(Json(SystemStatusResponse {
        database,
        services,
        tls,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn parses_openssl_enddate_format_and_computes_days_remaining() {
        let now = chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let result = days_remaining("Jan 11 00:00:00 2026 GMT", now);
        assert_eq!(result, Some(10));
    }

    #[test]
    fn past_expiry_yields_negative_days() {
        let now = chrono::Utc.with_ymd_and_hms(2026, 1, 11, 0, 0, 0).unwrap();
        let result = days_remaining("Jan  1 00:00:00 2026 GMT", now);
        assert_eq!(result, Some(-10));
    }

    #[test]
    fn malformed_input_yields_none() {
        assert_eq!(days_remaining("not a date", chrono::Utc::now()), None);
    }
}
