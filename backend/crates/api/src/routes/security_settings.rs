//! Rspamd-/ClamAV-Einstellungen (Spam-Schutz-/Virenschutz-Seiten im
//! Admin-Panel) — nur `super_admin` (`Action::ManageSystemSettings`, wie
//! `system.rs`). Postgres (`security_settings`, Singleton-Zeile) ist die
//! Quelle der Wahrheit; jede Änderung wird zu den vier betroffenen
//! Rspamd-Templates gerendert, per `rspamadm configtest` verifiziert und
//! erst danach live per Rspamd-Reload angewendet — schlägt der Configtest
//! fehl, bleiben die vorherigen Dateien unangetastet und die DB-Änderung
//! wird verworfen.

use crate::auth_extractor::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::{extract::State, http::HeaderMap, Json};
use havenmail_core::config_render::{
    render_security_settings, DomainRatelimitOverride, SecurityRenderContext,
};
use havenmail_core::rbac::Action;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Zielverzeichnis der gerenderten Rspamd-Dateien. Der `havenmail`-
/// Systembenutzer braucht dafür Schreibrechte (siehe
/// config/systemd/havenmail-api.service, ReadWritePaths) — bewusste,
/// dokumentierte Lockerung der sonst strikten Systemd-Härtung, siehe
/// docs/architecture.md.
const RSPAMD_LOCAL_D: &str = "/etc/rspamd/local.d";

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct SecuritySettings {
    pub spam_greylist_score: f32,
    pub spam_add_header_score: f32,
    pub spam_reject_score: f32,
    pub dmarc_enabled: bool,
    pub ratelimit_enabled: bool,
    pub ratelimit_per_hour: i32,
    pub ratelimit_burst: i32,
    pub antivirus_enabled: bool,
    pub antivirus_action: String,
    pub antivirus_max_size_mb: i32,
    pub min_password_length: i32,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

const SELECT_COLUMNS: &str = "spam_greylist_score, spam_add_header_score, spam_reject_score, \
     dmarc_enabled, ratelimit_enabled, ratelimit_per_hour, ratelimit_burst, \
     antivirus_enabled, antivirus_action, antivirus_max_size_mb, min_password_length, updated_at";

/// Aktuelle Mindestpasswortlänge — ersetzt die vier vormals hart codierten
/// `len() < 12`-Prüfungen in `routes/users.rs`. Ein einzelner `SELECT`
/// statt geteiltem In-Memory-Zustand, da Passwort-Änderungen nicht
/// performancekritisch sind und eine geänderte Richtlinie so sofort greift
/// (kein API-Neustart, kein Cache-Invalidieren nötig).
pub async fn min_password_length(pool: &sqlx::PgPool) -> ApiResult<i32> {
    let row: (i32,) =
        sqlx::query_as("SELECT min_password_length FROM security_settings WHERE id = 1")
            .fetch_one(pool)
            .await?;
    Ok(row.0)
}

#[derive(Debug, Serialize)]
pub struct PasswordPolicy {
    pub min_password_length: i32,
}

/// Anders als `get_settings` bewusst OHNE `ManageSystemSettings`-Gate —
/// jede eingeloggte Rolle setzt irgendwo ein Passwort (Selbstbedienung in
/// Account.tsx, Admin-Formulare in DomainDetail.tsx) und muss dafür die
/// aktuelle Mindestlänge kennen, um sie im Formular clientseitig
/// anzuzeigen/vorzuvalidieren. Das ist reine UX — die eigentliche
/// Durchsetzung passiert serverseitig in `routes/users.rs` über
/// `min_password_length()` oben, unabhängig davon, ob dieser Endpunkt
/// überhaupt aufgerufen wurde. Eine einzelne Ganzzahl ist zudem keine
/// sensible Information.
pub async fn get_password_policy(
    State(state): State<AppState>,
    AuthUser(_actor): AuthUser,
) -> ApiResult<Json<PasswordPolicy>> {
    Ok(Json(PasswordPolicy {
        min_password_length: min_password_length(&state.db).await?,
    }))
}

async fn fetch_settings(pool: &sqlx::PgPool) -> ApiResult<SecuritySettings> {
    Ok(sqlx::query_as(&format!(
        "SELECT {SELECT_COLUMNS} FROM security_settings WHERE id = 1"
    ))
    .fetch_one(pool)
    .await?)
}

pub async fn get_settings(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
) -> ApiResult<Json<SecuritySettings>> {
    if !actor.can(Action::ManageSystemSettings, None) {
        return Err(ApiError::Forbidden);
    }
    Ok(Json(fetch_settings(&state.db).await?))
}

#[derive(Debug, Deserialize)]
pub struct UpdateSpamSettingsRequest {
    pub spam_greylist_score: Option<f32>,
    pub spam_add_header_score: Option<f32>,
    pub spam_reject_score: Option<f32>,
    pub dmarc_enabled: Option<bool>,
    pub ratelimit_enabled: Option<bool>,
    pub ratelimit_per_hour: Option<i32>,
    pub ratelimit_burst: Option<i32>,
}

pub async fn update_spam_settings(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    headers: HeaderMap,
    Json(req): Json<UpdateSpamSettingsRequest>,
) -> ApiResult<Json<SecuritySettings>> {
    if !actor.can(Action::ManageSystemSettings, None) {
        return Err(ApiError::Forbidden);
    }
    let current = fetch_settings(&state.db).await?;

    let greylist = req
        .spam_greylist_score
        .unwrap_or(current.spam_greylist_score);
    let add_header = req
        .spam_add_header_score
        .unwrap_or(current.spam_add_header_score);
    let reject = req.spam_reject_score.unwrap_or(current.spam_reject_score);
    if !(greylist < add_header && add_header < reject) {
        return Err(ApiError::BadRequest(
            "Score-Schwellen müssen aufsteigend sein: greylist < add_header < reject".to_string(),
        ));
    }
    if let Some(rate) = req.ratelimit_per_hour {
        if rate < 1 {
            return Err(ApiError::BadRequest(
                "ratelimit_per_hour muss mindestens 1 sein".to_string(),
            ));
        }
    }

    let updated: SecuritySettings = sqlx::query_as(&format!(
        r#"
        UPDATE security_settings SET
            spam_greylist_score = $1, spam_add_header_score = $2, spam_reject_score = $3,
            dmarc_enabled = $4, ratelimit_enabled = $5, ratelimit_per_hour = $6, ratelimit_burst = $7,
            updated_at = now(), updated_by = $8
        WHERE id = 1
        RETURNING {SELECT_COLUMNS}
        "#
    ))
    .bind(greylist)
    .bind(add_header)
    .bind(reject)
    .bind(req.dmarc_enabled.unwrap_or(current.dmarc_enabled))
    .bind(req.ratelimit_enabled.unwrap_or(current.ratelimit_enabled))
    .bind(req.ratelimit_per_hour.unwrap_or(current.ratelimit_per_hour))
    .bind(req.ratelimit_burst.unwrap_or(current.ratelimit_burst))
    .bind(actor.user_id)
    .fetch_one(&state.db)
    .await?;

    apply_to_rspamd(&state).await?;

    crate::audit_log::log(
        &state,
        &actor,
        &headers,
        "security_settings.update_spam",
        "security_settings",
        None,
        serde_json::to_value(&current).ok(),
        serde_json::to_value(&updated).ok(),
    )
    .await;

    Ok(Json(updated))
}

#[derive(Debug, Deserialize)]
pub struct UpdateVirusSettingsRequest {
    pub antivirus_enabled: Option<bool>,
    pub antivirus_action: Option<String>,
    pub antivirus_max_size_mb: Option<i32>,
}

pub async fn update_virus_settings(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    headers: HeaderMap,
    Json(req): Json<UpdateVirusSettingsRequest>,
) -> ApiResult<Json<SecuritySettings>> {
    if !actor.can(Action::ManageSystemSettings, None) {
        return Err(ApiError::Forbidden);
    }
    let current = fetch_settings(&state.db).await?;

    let action = req
        .antivirus_action
        .unwrap_or(current.antivirus_action.clone());
    if !["reject", "add_header", "no_action"].contains(&action.as_str()) {
        return Err(ApiError::BadRequest(
            "antivirus_action muss reject, add_header oder no_action sein".to_string(),
        ));
    }
    let max_size = req
        .antivirus_max_size_mb
        .unwrap_or(current.antivirus_max_size_mb);
    if max_size < 1 {
        return Err(ApiError::BadRequest(
            "antivirus_max_size_mb muss mindestens 1 sein".to_string(),
        ));
    }

    let updated: SecuritySettings = sqlx::query_as(&format!(
        r#"
        UPDATE security_settings SET
            antivirus_enabled = $1, antivirus_action = $2, antivirus_max_size_mb = $3,
            updated_at = now(), updated_by = $4
        WHERE id = 1
        RETURNING {SELECT_COLUMNS}
        "#
    ))
    .bind(req.antivirus_enabled.unwrap_or(current.antivirus_enabled))
    .bind(&action)
    .bind(max_size)
    .bind(actor.user_id)
    .fetch_one(&state.db)
    .await?;

    apply_to_rspamd(&state).await?;

    crate::audit_log::log(
        &state,
        &actor,
        &headers,
        "security_settings.update_virus",
        "security_settings",
        None,
        serde_json::to_value(&current).ok(),
        serde_json::to_value(&updated).ok(),
    )
    .await;

    Ok(Json(updated))
}

#[derive(Debug, Deserialize)]
pub struct UpdatePasswordPolicyRequest {
    pub min_password_length: i32,
}

/// Anders als die Spam-/Virus-Settings hat die Passwort-Richtlinie keine
/// Entsprechung in einer Rspamd-Konfigurationsdatei — reines DB-Feld, das
/// bei jeder Passwort-Prüfung frisch gelesen wird (`min_password_length()`
/// oben), daher hier kein `apply_to_rspamd`-Aufruf nötig.
pub async fn update_password_policy(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    headers: HeaderMap,
    Json(req): Json<UpdatePasswordPolicyRequest>,
) -> ApiResult<Json<SecuritySettings>> {
    if !actor.can(Action::ManageSystemSettings, None) {
        return Err(ApiError::Forbidden);
    }
    if req.min_password_length < 8 {
        return Err(ApiError::BadRequest(
            "min_password_length muss mindestens 8 sein".to_string(),
        ));
    }
    let current = fetch_settings(&state.db).await?;

    let updated: SecuritySettings = sqlx::query_as(&format!(
        r#"
        UPDATE security_settings SET
            min_password_length = $1,
            updated_at = now(), updated_by = $2
        WHERE id = 1
        RETURNING {SELECT_COLUMNS}
        "#
    ))
    .bind(req.min_password_length)
    .bind(actor.user_id)
    .fetch_one(&state.db)
    .await?;

    crate::audit_log::log(
        &state,
        &actor,
        &headers,
        "security_settings.update_password_policy",
        "security_settings",
        None,
        serde_json::to_value(&current).ok(),
        serde_json::to_value(&updated).ok(),
    )
    .await;

    Ok(Json(updated))
}

#[derive(Debug, FromRow)]
struct DomainRatelimitOverrideRow {
    name: String,
    per_hour: Option<i32>,
    burst: Option<i32>,
}

/// Domains mit einem gesetzten Rate-Limit-Override (siehe routes/domains.rs)
/// — bereits auf effektive Werte aufgelöst: fehlt eines der beiden
/// DB-Felder (z. B. nur `per_hour` gesetzt), wird für das andere der
/// gerade aktuelle globale Wert eingesetzt, damit das Template selbst
/// keine Fallback-Logik nachbilden muss.
async fn fetch_domain_ratelimit_overrides(
    pool: &sqlx::PgPool,
    settings: &SecuritySettings,
) -> ApiResult<Vec<DomainRatelimitOverride>> {
    let rows: Vec<DomainRatelimitOverrideRow> = sqlx::query_as(
        r#"
        SELECT name,
               ratelimit_per_hour_override as per_hour,
               ratelimit_burst_override as burst
        FROM domains
        WHERE ratelimit_per_hour_override IS NOT NULL OR ratelimit_burst_override IS NOT NULL
        ORDER BY name
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| DomainRatelimitOverride {
            name: r.name,
            per_hour: r.per_hour.unwrap_or(settings.ratelimit_per_hour),
            burst: r.burst.unwrap_or(settings.ratelimit_burst),
        })
        .collect())
}

/// Liest den aktuellen Stand aus `security_settings` UND `domains`
/// (Rate-Limit-Overrides) frisch aus der DB, rendert die vier betroffenen
/// Rspamd-Templates neu, schreibt sie nach `/etc/rspamd/local.d/`, prüft
/// mit `rspamadm configtest` und stößt bei Erfolg einen Reload an. Bei
/// einem Configtest-Fehler werden die zuvor gesicherten Dateiinhalte
/// wiederhergestellt und der Aufruf schlägt mit `400` fehl — die
/// DB-Änderung des jeweiligen Aufrufers ist zu diesem Zeitpunkt zwar schon
/// aktiv, das ist bewusst: der nächste erfolgreiche Save rendert ohnehin
/// neu, und ein inkonsistenter Zwischenzustand zwischen DB und Live-Config
/// ist unkritisch, da die DB stets die Quelle der Wahrheit ist.
///
/// Nimmt bewusst keine Einstellungen als Parameter entgegen (liest immer
/// frisch), damit sowohl `update_spam_settings`/`update_virus_settings`
/// hier als auch `routes/domains.rs`s Rate-Limit-Override-Endpunkt
/// denselben, immer konsistenten Gesamtzustand rendern — die
/// domainbezogenen Buckets im ratelimit.conf-Template hängen von BEIDEN
/// Quellen zugleich ab (globaler Default UND Domain-Overrides).
pub(crate) async fn apply_to_rspamd(state: &AppState) -> ApiResult<()> {
    let settings = fetch_settings(&state.db).await?;
    let domain_overrides = fetch_domain_ratelimit_overrides(&state.db, &settings).await?;
    let ctx = SecurityRenderContext {
        spam_greylist_score: settings.spam_greylist_score,
        spam_add_header_score: settings.spam_add_header_score,
        spam_reject_score: settings.spam_reject_score,
        dmarc_enabled: settings.dmarc_enabled,
        ratelimit_enabled: settings.ratelimit_enabled,
        ratelimit_per_hour: settings.ratelimit_per_hour,
        ratelimit_burst: settings.ratelimit_burst,
        antivirus_enabled: settings.antivirus_enabled,
        antivirus_action: settings.antivirus_action.clone(),
        antivirus_max_size_mb: settings.antivirus_max_size_mb,
        domain_overrides,
    };
    let rendered = render_security_settings(&state.config_dir, &ctx)
        .map_err(|e| ApiError::BadRequest(format!("Template-Fehler: {e}")))?;

    let mut backups = Vec::with_capacity(rendered.len());
    for (template_name, content) in &rendered {
        let out_path = output_path(template_name);
        let previous = std::fs::read_to_string(&out_path).ok();
        backups.push((out_path.clone(), previous));
        std::fs::write(&out_path, content)
            .map_err(|e| ApiError::BadRequest(format!("Konnte {out_path} nicht schreiben: {e}")))?;
    }

    let configtest = tokio::process::Command::new("rspamadm")
        .arg("configtest")
        .output()
        .await;

    let configtest_ok = matches!(&configtest, Ok(output) if output.status.success());
    if !configtest_ok {
        for (path, previous) in &backups {
            match previous {
                Some(content) => {
                    let _ = std::fs::write(path, content);
                }
                None => {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
        let detail = match configtest {
            Ok(output) => String::from_utf8_lossy(&output.stderr).trim().to_string(),
            Err(e) => e.to_string(),
        };
        return Err(ApiError::BadRequest(format!(
            "Rspamd-Konfiguration ungültig, Änderung verworfen: {detail}"
        )));
    }

    // Rspamds Controller-API bietet keinen HTTP-Reload-Endpunkt (live
    // geprüft, siehe rspamd_client.rs) — `systemctl reload rspamd` direkt
    // aus diesem Prozess scheitert an NoNewPrivileges (blockiert jede Art
    // von Rechte-Eskalation, auch über sudo/setuid — dieses Flag bewusst
    // NICHT gelockert, da es breiten Schutz bietet). Stattdessen schreibt
    // dieser Prozess (User "havenmail", Schreibrecht bereits über
    // /var/lib/havenmail in ReadWritePaths) nur eine Trigger-Datei; ein
    // separates, unsandboxed systemd-Path-Unit (als root, siehe
    // config/systemd/havenmail-rspamd-reload.{service,path}) übernimmt
    // den eigentlichen Reload — keine weitere Härtungs-Lockerung nötig.
    let state_dir =
        std::env::var("HAVENMAIL_STATE_DIR").unwrap_or_else(|_| "/var/lib/havenmail".to_string());
    let trigger_path = std::path::PathBuf::from(format!("{state_dir}/rspamd-reload-trigger"));
    havenmail_core::trigger_file::write(&trigger_path, &chrono::Utc::now().to_rfc3339())
        .map_err(|e| ApiError::BadRequest(format!("Rspamd-Reload-Trigger fehlgeschlagen: {e}")))?;

    Ok(())
}

/// "rspamd/local.d/actions.conf.tera" (Pfad relativ zum config-Repo-
/// Verzeichnis) -> "/etc/rspamd/local.d/actions.conf" (Ziel auf der Platte).
fn output_path(template_name: &str) -> String {
    let file_name = template_name
        .rsplit('/')
        .next()
        .unwrap_or(template_name)
        .trim_end_matches(".tera");
    format!("{RSPAMD_LOCAL_D}/{file_name}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_output_path_from_template_name() {
        assert_eq!(
            output_path("rspamd/local.d/actions.conf.tera"),
            "/etc/rspamd/local.d/actions.conf"
        );
    }
}
