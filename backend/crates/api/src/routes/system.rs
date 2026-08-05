//! System-/Dienststatus für die Admin-Oberfläche — nur `super_admin`
//! (`Action::ManageSystemSettings`). Zeigt, ob die orchestrierten
//! Mail-Engines tatsächlich laufen, nicht nur ob die Control-Plane-API
//! selbst erreichbar ist (das prüft bereits `/healthz`/`/readyz`).

use crate::auth_extractor::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::{
    extract::{Query, State},
    Json,
};
use havenmail_core::rbac::Action;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

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

#[derive(Debug, FromRow)]
struct MetricsSnapshotRow {
    captured_at: chrono::DateTime<chrono::Utc>,
    rspamd_scanned: Option<i64>,
    rspamd_spam_count: Option<i64>,
    rspamd_ham_count: Option<i64>,
    rspamd_action_reject: Option<i64>,
    clamav_detected_since_last: Option<i32>,
    mail_queue_size: Option<i32>,
    disk_used_percent: Option<f32>,
}

#[derive(Debug, Serialize)]
pub struct MetricsPoint {
    pub captured_at: chrono::DateTime<chrono::Utc>,
    /// `None` für den allerersten Punkt im Zeitraum (kein Vorgänger für
    /// eine Delta-Berechnung) oder wenn einer der beiden Rohwerte fehlte.
    pub spam_delta: Option<i64>,
    pub ham_delta: Option<i64>,
    pub scanned_delta: Option<i64>,
    pub reject_delta: Option<i64>,
    /// Bereits ein Delta zum Zeitpunkt der Messung, siehe
    /// `havenmail-cli snapshot-metrics`.
    pub virus_detected: Option<i32>,
    pub mail_queue_size: Option<i32>,
    pub disk_used_percent: Option<f32>,
}

#[derive(Debug, Deserialize)]
pub struct MetricsQuery {
    #[serde(default = "default_range")]
    pub range: String,
}

fn default_range() -> String {
    "7d".to_string()
}

fn range_to_interval_days(range: &str) -> i64 {
    match range {
        "30d" => 30,
        _ => 7,
    }
}

/// Bildet Deltas zwischen aufeinanderfolgenden Snapshots für die
/// kumulativen Rspamd-Zähler — Aufrufer (Dashboard-Charts) wollen "wie
/// viel Spam kam in diesem Zeitfenster rein", nicht den rohen
/// Seit-rspamd-Start-Zähler.
fn compute_deltas(rows: Vec<MetricsSnapshotRow>) -> Vec<MetricsPoint> {
    let mut points = Vec::with_capacity(rows.len());
    let mut prev: Option<&MetricsSnapshotRow> = None;
    for row in &rows {
        let delta = |current: Option<i64>, previous: Option<i64>| match (current, previous) {
            (Some(c), Some(p)) if c >= p => Some(c - p),
            _ => None,
        };
        points.push(MetricsPoint {
            captured_at: row.captured_at,
            spam_delta: prev.and_then(|p| delta(row.rspamd_spam_count, p.rspamd_spam_count)),
            ham_delta: prev.and_then(|p| delta(row.rspamd_ham_count, p.rspamd_ham_count)),
            scanned_delta: prev.and_then(|p| delta(row.rspamd_scanned, p.rspamd_scanned)),
            reject_delta: prev
                .and_then(|p| delta(row.rspamd_action_reject, p.rspamd_action_reject)),
            virus_detected: row.clamav_detected_since_last,
            mail_queue_size: row.mail_queue_size,
            disk_used_percent: row.disk_used_percent,
        });
        prev = Some(row);
    }
    points
}

/// Verlaufsdaten für die Dashboard-Charts (Spam-Schutz, Warteschlange,
/// Speicherauslastung). `range` ist `7d` (Default) oder `30d`. Punkte
/// entstehen alle 15 Minuten durch `havenmail-cli snapshot-metrics`
/// (systemd-Timer, siehe config/systemd/havenmail-metrics-snapshot.timer)
/// — direkt nach der Aktivierung sind entsprechend erst wenige Punkte
/// vorhanden, ein aussagekräftiger Verlauf braucht etwas Anlaufzeit.
pub async fn system_metrics(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Query(query): Query<MetricsQuery>,
) -> ApiResult<Json<Vec<MetricsPoint>>> {
    if !actor.can(Action::ManageSystemSettings, None) {
        return Err(ApiError::Forbidden);
    }
    let days = range_to_interval_days(&query.range);

    let rows: Vec<MetricsSnapshotRow> = sqlx::query_as(
        r#"
        SELECT captured_at, rspamd_scanned, rspamd_spam_count, rspamd_ham_count,
               rspamd_action_reject, clamav_detected_since_last, mail_queue_size, disk_used_percent
        FROM metrics_snapshots
        WHERE captured_at > now() - make_interval(days => $1)
        ORDER BY captured_at ASC
        "#,
    )
    .bind(days as i32)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(compute_deltas(rows)))
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

    fn snapshot_row(
        minute: u32,
        scanned: i64,
        spam: i64,
        ham: i64,
    ) -> MetricsSnapshotRow {
        MetricsSnapshotRow {
            captured_at: chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, minute, 0).unwrap(),
            rspamd_scanned: Some(scanned),
            rspamd_spam_count: Some(spam),
            rspamd_ham_count: Some(ham),
            rspamd_action_reject: Some(0),
            clamav_detected_since_last: Some(0),
            mail_queue_size: Some(2),
            disk_used_percent: Some(12.5),
        }
    }

    #[test]
    fn first_point_has_no_delta() {
        let points = compute_deltas(vec![snapshot_row(0, 10, 1, 9)]);
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].spam_delta, None);
    }

    #[test]
    fn computes_delta_between_consecutive_cumulative_counters() {
        let points = compute_deltas(vec![snapshot_row(0, 10, 1, 9), snapshot_row(15, 25, 4, 21)]);
        assert_eq!(points[1].spam_delta, Some(3));
        assert_eq!(points[1].ham_delta, Some(12));
        assert_eq!(points[1].scanned_delta, Some(15));
    }

    #[test]
    fn rspamd_restart_resetting_counters_yields_none_instead_of_negative() {
        // Rspamd-Neustart setzt kumulative Zähler zurück auf 0 — ein
        // negatives Delta wäre irreführend im Chart, also None statt
        // eines falschen negativen Werts.
        let points = compute_deltas(vec![snapshot_row(0, 100, 10, 90), snapshot_row(15, 5, 1, 4)]);
        assert_eq!(points[1].spam_delta, None);
    }
}
