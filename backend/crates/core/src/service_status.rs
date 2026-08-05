//! Dienststatus der orchestrierten Mail-Engines, gemeinsam genutzt von der
//! System-Status-Route (`api::routes::system`) und dem periodischen
//! Benachrichtigungs-Check (`havenmail-cli notify-check`).

use serde::Serialize;

/// Dienste, die der Installer orchestriert (siehe
/// scripts/lib/install_steps.sh, havenmail_start_services). `nginx` und
/// `havenmail-api` bewusst mit aufgeführt — ein Reload/Neustart, der die
/// eigene Erreichbarkeit nicht beeinträchtigt hat, soll trotzdem sichtbar
/// sein.
pub const MANAGED_UNITS: &[&str] = &[
    "havenmail-api",
    "postfix",
    "dovecot",
    "rspamd",
    "clamav-daemon",
    "nginx",
    "fail2ban",
];

#[derive(Debug, Clone, Serialize)]
pub struct ServiceStatus {
    pub unit: String,
    /// `true`, wenn `systemctl is-active` "active" meldet. `false` deckt
    /// sowohl "inactive"/"failed" als auch den Fall ab, dass systemctl
    /// selbst nicht aufgerufen werden konnte (z. B. lokale Entwicklung
    /// ohne systemd) — dort steht `detail` auf "unknown".
    pub active: bool,
    pub detail: String,
}

pub async fn query_unit_status(unit: &str) -> ServiceStatus {
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
