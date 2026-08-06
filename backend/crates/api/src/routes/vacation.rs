//! Abwesenheitsnotiz je Postfach (Sieve-"vacation"-Autoresponder).
//! Dovecot Pigeonhole ist installiert, aber das Sieve-Plugin war für die
//! LMTP-Zustellung bisher auskommentiert (siehe
//! config/dovecot/21-havenmail-lmtp.conf.tera) — kein ManageSieve-Zugang
//! aktiv, Nutzer bearbeiten kein Skript direkt. Postgres
//! (`vacation_responders`) ist die Quelle der Wahrheit, das
//! `.dovecot.sieve`-Skript im Mailbox-Home ist nur eine daraus gerenderte
//! Ableitung (analog zu security_settings -> Rspamd-Configs).
//!
//! Selbstbedienung + Domain-Verwaltung: derselbe is_self/is_domain_manager-
//! Zugriffsschnitt wie bei `routes/users.rs::update_user`.

use crate::auth_extractor::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    Json,
};
use chrono::NaiveDate;
use havenmail_core::rbac::Action;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct VacationResponder {
    pub enabled: bool,
    pub subject: String,
    pub message: String,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Zeilenbegrenzung, damit ein absurd langer Nachrichtentext nicht zu
/// einem unnötig großen Sieve-Skript führt (Pigeonhole hat ohnehin ein
/// eigenes `sieve_max_script_size`-Limit, das hier deutlich unterschritten
/// bleibt).
const MAX_MESSAGE_LEN: usize = 8000;
const MAX_SUBJECT_LEN: usize = 200;

#[derive(Debug, FromRow)]
struct UserMailboxInfo {
    domain_id: Uuid,
    local_part: String,
    domain_name: String,
}

async fn fetch_mailbox_info(pool: &sqlx::PgPool, user_id: Uuid) -> ApiResult<UserMailboxInfo> {
    sqlx::query_as(
        r#"
        SELECT u.domain_id, u.local_part, d.name as domain_name
        FROM users u JOIN domains d ON d.id = u.domain_id
        WHERE u.id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .ok_or(ApiError::NotFound)
}

fn check_access(actor: &havenmail_core::Actor, user_id: Uuid, domain_id: Uuid) -> ApiResult<()> {
    let is_self = actor.owns(user_id);
    let is_domain_manager = actor.can(Action::ManageDomainUsers, Some(domain_id));
    if !is_self && !is_domain_manager {
        return Err(ApiError::NotFound);
    }
    Ok(())
}

async fn fetch_vacation(
    state: &AppState,
    actor: &havenmail_core::Actor,
    user_id: Uuid,
) -> ApiResult<VacationResponder> {
    let info = fetch_mailbox_info(&state.db, user_id).await?;
    check_access(actor, user_id, info.domain_id)?;

    let existing: Option<VacationResponder> = sqlx::query_as(
        "SELECT enabled, subject, message, start_date, end_date, updated_at \
         FROM vacation_responders WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?;

    Ok(existing.unwrap_or(VacationResponder {
        enabled: false,
        subject: "Automatische Abwesenheitsnotiz".to_string(),
        message: String::new(),
        start_date: None,
        end_date: None,
        updated_at: chrono::Utc::now(),
    }))
}

pub async fn get_vacation(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(user_id): Path<Uuid>,
) -> ApiResult<Json<VacationResponder>> {
    Ok(Json(fetch_vacation(&state, &actor, user_id).await?))
}

/// Selbstbedienungs-Variante — löst die eigene User-ID aus dem JWT auf
/// statt über einen `:user_id`-Pfadparameter (das Frontend kennt die
/// eigene User-ID sonst nicht, kein Client-seitiges JWT-Decoding, siehe
/// api.ts und `users::change_own_password`-Analogie).
pub async fn get_own_vacation(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
) -> ApiResult<Json<VacationResponder>> {
    let user_id = actor.user_id;
    Ok(Json(fetch_vacation(&state, &actor, user_id).await?))
}

#[derive(Debug, Deserialize)]
pub struct UpdateVacationRequest {
    pub enabled: bool,
    pub subject: String,
    pub message: String,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
}

async fn save_vacation(
    state: &AppState,
    actor: &havenmail_core::Actor,
    headers: &axum::http::HeaderMap,
    user_id: Uuid,
    req: UpdateVacationRequest,
) -> ApiResult<VacationResponder> {
    let info = fetch_mailbox_info(&state.db, user_id).await?;
    check_access(actor, user_id, info.domain_id)?;

    if req.subject.trim().is_empty() || req.subject.len() > MAX_SUBJECT_LEN {
        return Err(ApiError::BadRequest(format!(
            "Betreff muss zwischen 1 und {MAX_SUBJECT_LEN} Zeichen lang sein"
        )));
    }
    if req.message.len() > MAX_MESSAGE_LEN {
        return Err(ApiError::BadRequest(format!(
            "Nachricht darf höchstens {MAX_MESSAGE_LEN} Zeichen lang sein"
        )));
    }
    if let (Some(s), Some(e)) = (req.start_date, req.end_date) {
        if e < s {
            return Err(ApiError::BadRequest(
                "Enddatum darf nicht vor dem Startdatum liegen".to_string(),
            ));
        }
    }

    let updated: VacationResponder = sqlx::query_as(
        r#"
        INSERT INTO vacation_responders (user_id, enabled, subject, message, start_date, end_date, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, now())
        ON CONFLICT (user_id) DO UPDATE SET
            enabled = $2, subject = $3, message = $4, start_date = $5, end_date = $6, updated_at = now()
        RETURNING enabled, subject, message, start_date, end_date, updated_at
        "#,
    )
    .bind(user_id)
    .bind(req.enabled)
    .bind(&req.subject)
    .bind(&req.message)
    .bind(req.start_date)
    .bind(req.end_date)
    .fetch_one(&state.db)
    .await?;

    let address = format!("{}@{}", info.local_part, info.domain_name);
    apply_to_mailbox(&info.domain_name, &info.local_part, &address, &updated).await?;

    crate::audit_log::log(
        state,
        actor,
        headers,
        "vacation.update",
        &user_id.to_string(),
        Some(info.domain_id),
        None,
        serde_json::to_value(&updated).ok(),
    )
    .await;

    Ok(updated)
}

pub async fn update_vacation(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(user_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
    Json(req): Json<UpdateVacationRequest>,
) -> ApiResult<Json<VacationResponder>> {
    Ok(Json(
        save_vacation(&state, &actor, &headers, user_id, req).await?,
    ))
}

/// Selbstbedienungs-Variante, analog zu `get_own_vacation`.
pub async fn update_own_vacation(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    headers: axum::http::HeaderMap,
    Json(req): Json<UpdateVacationRequest>,
) -> ApiResult<Json<VacationResponder>> {
    let user_id = actor.user_id;
    Ok(Json(
        save_vacation(&state, &actor, &headers, user_id, req).await?,
    ))
}

/// Zielverzeichnis der Mailboxen — dieselbe Env-Variable/Konvention wie
/// `mailbox_storage`/`get_users_storage` in `routes/users.rs`.
fn mail_base() -> String {
    std::env::var("HAVENMAIL_MAIL_DIR").unwrap_or_else(|_| "/var/mail/havenmail".to_string())
}

/// Schreibt (bzw. entfernt) das `.dovecot.sieve`-Skript im Mailbox-Home
/// und kompiliert es per `sievec` — dasselbe Sicherheitsnetz-Muster wie
/// `security_settings::apply_to_rspamd`: vorherigen Inhalt sichern, neu
/// schreiben, mit einem externen Tool verifizieren, bei Fehlschlag den
/// alten Zustand wiederherstellen statt eine kaputte/halbe Datei stehen zu
/// lassen. `sievec` braucht Lesezugriff auf die volle Dovecot-Config
/// (u. a. `10-auth-sql.conf`, `root:dovecot 0640`) — der `havenmail`-
/// Systembenutzer ist deshalb Mitglied der Gruppe `dovecot` (siehe
/// scripts/lib/common.sh), sonst schlägt der Compile-Schritt mit
/// "Permission denied" fehl (live geprüft).
async fn apply_to_mailbox(
    domain_name: &str,
    local_part: &str,
    address: &str,
    settings: &VacationResponder,
) -> ApiResult<()> {
    let home = std::path::PathBuf::from(mail_base())
        .join(domain_name)
        .join(local_part);
    let sieve_path = home.join(".dovecot.sieve");
    let svbin_path = home.join(".dovecot.svbin");

    if !settings.enabled {
        // Deaktivieren = Skript entfernen, keine leere/inaktive Datei
        // liegen lassen — Pigeonhole liefert ohne aktives Skript ganz
        // normal zu, keine Sonderbehandlung nötig.
        let _ = std::fs::remove_file(&sieve_path);
        let _ = std::fs::remove_file(&svbin_path);
        return Ok(());
    }

    // Verzeichnis kann fehlen, wenn das Postfach noch nie eine Zustellung
    // oder ein IMAP-Login hatte (Dovecot legt es sonst selbst an).
    std::fs::create_dir_all(&home)
        .map_err(|e| ApiError::BadRequest(format!("Mailbox-Verzeichnis fehlt: {e}")))?;

    let script = havenmail_core::sieve_render::render_vacation_script_with_range(
        address,
        &settings.subject,
        &settings.message,
        settings.start_date,
        settings.end_date,
    );

    let previous_sieve = std::fs::read_to_string(&sieve_path).ok();
    let previous_svbin_existed = svbin_path.exists();
    std::fs::write(&sieve_path, &script)
        .map_err(|e| ApiError::BadRequest(format!("Konnte Sieve-Skript nicht schreiben: {e}")))?;

    let compile = tokio::process::Command::new("sievec")
        .arg(&sieve_path)
        .output()
        .await;
    let compile_ok = matches!(&compile, Ok(output) if output.status.success());

    if !compile_ok {
        match previous_sieve {
            Some(content) => {
                let _ = std::fs::write(&sieve_path, content);
            }
            None => {
                let _ = std::fs::remove_file(&sieve_path);
            }
        }
        if !previous_svbin_existed {
            let _ = std::fs::remove_file(&svbin_path);
        }
        let detail = match compile {
            Ok(output) => String::from_utf8_lossy(&output.stderr).trim().to_string(),
            Err(e) => e.to_string(),
        };
        return Err(ApiError::BadRequest(format!(
            "Sieve-Skript ungültig, Änderung verworfen: {detail}"
        )));
    }

    Ok(())
}
