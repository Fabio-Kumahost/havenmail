use crate::auth_extractor::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use havenmail_core::rbac::{Action, Role};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, FromRow, Serialize)]
pub struct Domain {
    pub id: Uuid,
    pub name: String,
    pub is_active: bool,
    pub catch_all_enabled: bool,
    pub catch_all_target: Option<String>,
    pub quota_bytes: Option<i64>,
    /// `None` = kein Override, Domain nutzt das globale Rate-Limit aus
    /// `security_settings` (siehe routes/security_settings.rs). Editierbar
    /// über den eigenen `PATCH .../ratelimit-override`-Endpunkt unten, nicht
    /// über das allgemeine `update_domain` (siehe dortiger Kommentar zum
    /// fehlenden Clear-auf-NULL-Mechanismus).
    pub ratelimit_per_hour_override: Option<i32>,
    pub ratelimit_burst_override: Option<i32>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Wiederverwendete Spaltenliste für alle SELECT/RETURNING-Stellen dieser
/// Datei — ein einziger Ort, an dem `Domain`s Felder mit der DB-Query in
/// Sync gehalten werden.
const DOMAIN_COLUMNS: &str = "id, name, is_active, catch_all_enabled, catch_all_target, \
     quota_bytes, ratelimit_per_hour_override, ratelimit_burst_override, created_at";

#[derive(Debug, Deserialize)]
pub struct CreateDomainRequest {
    pub name: String,
    pub quota_bytes: Option<i64>,
}

/// Nur `super_admin` darf neue Domains anlegen — Domain-Erstellung ist eine
/// systemweite Ressourcenzuteilung, kein Domain-scoped-Vorgang.
pub async fn create_domain(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    headers: HeaderMap,
    Json(req): Json<CreateDomainRequest>,
) -> ApiResult<Json<Domain>> {
    if actor.role != Role::SuperAdmin {
        return Err(ApiError::Forbidden);
    }
    if req.name.trim().is_empty() || !req.name.contains('.') {
        return Err(ApiError::BadRequest("ungültiger Domain-Name".to_string()));
    }

    let domain: Domain = sqlx::query_as(&format!(
        r#"
        INSERT INTO domains (name, quota_bytes)
        VALUES ($1, $2)
        RETURNING {DOMAIN_COLUMNS}
        "#
    ))
    .bind(req.name.trim().to_lowercase())
    .bind(req.quota_bytes)
    .fetch_one(&state.db)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
            ApiError::Conflict("Domain existiert bereits".to_string())
        }
        _ => ApiError::Internal(e),
    })?;

    crate::audit_log::log(
        &state,
        &actor,
        &headers,
        "domain.create",
        &domain.id.to_string(),
        Some(domain.id),
        None,
        serde_json::to_value(&domain).ok(),
    )
    .await;

    Ok(Json(domain))
}

/// `super_admin` sieht alle Domains, `domain_admin`/`user` nur die eigene.
pub async fn list_domains(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
) -> ApiResult<Json<Vec<Domain>>> {
    let domains: Vec<Domain> = match actor.role {
        Role::SuperAdmin => {
            sqlx::query_as(&format!(
                "SELECT {DOMAIN_COLUMNS} FROM domains ORDER BY name"
            ))
            .fetch_all(&state.db)
            .await?
        }
        Role::DomainAdmin | Role::User => {
            let Some(domain_id) = actor.domain_id else {
                return Ok(Json(vec![]));
            };
            sqlx::query_as(&format!(
                "SELECT {DOMAIN_COLUMNS} FROM domains WHERE id = $1"
            ))
            .bind(domain_id)
            .fetch_all(&state.db)
            .await?
        }
    };
    Ok(Json(domains))
}

pub async fn get_domain(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(domain_id): Path<Uuid>,
) -> ApiResult<Json<Domain>> {
    if !actor.can(Action::ManageDomain, Some(domain_id)) {
        return Err(ApiError::NotFound); // kein Hinweis auf Existenz fremder Domains
    }
    let domain: Domain = sqlx::query_as(&format!(
        "SELECT {DOMAIN_COLUMNS} FROM domains WHERE id = $1"
    ))
    .bind(domain_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound)?;
    Ok(Json(domain))
}

#[derive(Debug, Deserialize)]
pub struct UpdateDomainRequest {
    pub is_active: Option<bool>,
    pub catch_all_enabled: Option<bool>,
    pub catch_all_target: Option<String>,
    pub quota_bytes: Option<i64>,
}

pub async fn update_domain(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(domain_id): Path<Uuid>,
    headers: HeaderMap,
    Json(req): Json<UpdateDomainRequest>,
) -> ApiResult<Json<Domain>> {
    if !actor.can(Action::ManageDomain, Some(domain_id)) {
        return Err(ApiError::NotFound);
    }
    if req.catch_all_enabled == Some(true) && req.catch_all_target.is_none() {
        return Err(ApiError::BadRequest(
            "catch_all_target erforderlich, wenn catch_all_enabled=true".to_string(),
        ));
    }

    let current: Domain = sqlx::query_as(&format!(
        "SELECT {DOMAIN_COLUMNS} FROM domains WHERE id = $1"
    ))
    .bind(domain_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound)?;
    let current_snapshot = serde_json::to_value(&current).ok();

    let is_active = req.is_active.unwrap_or(current.is_active);
    let catch_all_enabled = req.catch_all_enabled.unwrap_or(current.catch_all_enabled);
    let catch_all_target = req.catch_all_target.or(current.catch_all_target);
    let quota_bytes = req.quota_bytes.or(current.quota_bytes);

    let domain: Domain = sqlx::query_as(&format!(
        r#"
        UPDATE domains
        SET is_active = $2, catch_all_enabled = $3, catch_all_target = $4, quota_bytes = $5
        WHERE id = $1
        RETURNING {DOMAIN_COLUMNS}
        "#
    ))
    .bind(domain_id)
    .bind(is_active)
    .bind(catch_all_enabled)
    .bind(&catch_all_target)
    .bind(quota_bytes)
    .fetch_one(&state.db)
    .await?;

    crate::audit_log::log(
        &state,
        &actor,
        &headers,
        "domain.update",
        &domain_id.to_string(),
        Some(domain_id),
        current_snapshot,
        serde_json::to_value(&domain).ok(),
    )
    .await;

    Ok(Json(domain))
}

/// Beide Felder sind bewusst nicht optional (kein `#[serde(default)]`) und
/// müssen daher in jeder Anfrage explizit als Zahl oder `null` mitgeschickt
/// werden — anders als `UpdateDomainRequest::quota_bytes` etc., das über
/// `.or(current…)` NIE auf NULL zurückgesetzt werden kann (fehlendes Feld
/// heißt dort "unverändert"), muss dieser Endpunkt einen Override auch
/// wieder löschen können ("leeres Feld im Frontend" = zurück zum globalen
/// Default). Ein eigener, kleiner Endpunkt statt eine Ausnahme im
/// allgemeinen `update_domain` einzubauen hält diese Tri-State-Semantik
/// (unverändert/gesetzt/gelöscht) an einer einzigen, klar benannten Stelle.
#[derive(Debug, Deserialize)]
pub struct UpdateRatelimitOverrideRequest {
    pub ratelimit_per_hour_override: Option<i32>,
    pub ratelimit_burst_override: Option<i32>,
}

pub async fn update_ratelimit_override(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(domain_id): Path<Uuid>,
    headers: HeaderMap,
    Json(req): Json<UpdateRatelimitOverrideRequest>,
) -> ApiResult<Json<Domain>> {
    if !actor.can(Action::ManageDomain, Some(domain_id)) {
        return Err(ApiError::NotFound);
    }
    if let Some(v) = req.ratelimit_per_hour_override {
        if v < 1 {
            return Err(ApiError::BadRequest(
                "ratelimit_per_hour_override muss mindestens 1 sein".to_string(),
            ));
        }
    }
    if let Some(v) = req.ratelimit_burst_override {
        if v < 1 {
            return Err(ApiError::BadRequest(
                "ratelimit_burst_override muss mindestens 1 sein".to_string(),
            ));
        }
    }

    let current: Domain = sqlx::query_as(&format!(
        "SELECT {DOMAIN_COLUMNS} FROM domains WHERE id = $1"
    ))
    .bind(domain_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound)?;
    let current_snapshot = serde_json::to_value(&current).ok();

    let domain: Domain = sqlx::query_as(&format!(
        r#"
        UPDATE domains
        SET ratelimit_per_hour_override = $2, ratelimit_burst_override = $3
        WHERE id = $1
        RETURNING {DOMAIN_COLUMNS}
        "#
    ))
    .bind(domain_id)
    .bind(req.ratelimit_per_hour_override)
    .bind(req.ratelimit_burst_override)
    .fetch_one(&state.db)
    .await?;

    // Betrifft dasselbe gerenderte ratelimit.conf wie die globalen
    // Spam-Settings (siehe security_settings::apply_to_rspamd — liest
    // Domain-Overrides IMMER frisch aus der DB, unabhängig davon, von wo
    // aus die Änderung kam).
    crate::routes::security_settings::apply_to_rspamd(&state).await?;

    crate::audit_log::log(
        &state,
        &actor,
        &headers,
        "domain.update_ratelimit_override",
        &domain_id.to_string(),
        Some(domain_id),
        current_snapshot,
        serde_json::to_value(&domain).ok(),
    )
    .await;

    Ok(Json(domain))
}

/// Löschen ist destruktiv (kaskadiert auf Benutzer/Aliase) — bewusst nur
/// `super_admin`, unabhängig vom sonst für ManageDomain reichenden Scope.
pub async fn delete_domain(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(domain_id): Path<Uuid>,
    headers: HeaderMap,
) -> ApiResult<Json<serde_json::Value>> {
    if actor.role != Role::SuperAdmin {
        return Err(ApiError::Forbidden);
    }
    let result = sqlx::query("DELETE FROM domains WHERE id = $1")
        .bind(domain_id)
        .execute(&state.db)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }

    crate::audit_log::log(
        &state,
        &actor,
        &headers,
        "domain.delete",
        &domain_id.to_string(),
        None, // Domain bereits gelöscht — kein gültiger FK-Wert mehr
        None,
        None,
    )
    .await;

    Ok(Json(serde_json::json!({ "status": "deleted" })))
}

/// Aggregierte Domain-Übersicht fürs Reseller-/Mandanten-Dashboard: alle
/// Domains nebeneinander mit Nutzeranzahl und realer Speichernutzung.
/// Ein eigener Endpunkt statt `list_domains` + N Einzelabfragen im
/// Frontend — sowohl die Nutzerzahl (eine gruppierte SQL-Abfrage statt
/// N) als auch `du -sb` je Domain-Verzeichnis (statt je Postfach, wie
/// `get_users_storage` es für eine einzelne Domain tut) sind hier auf
/// Domain-Ebene zusammengefasst, damit die Übersicht auch bei vielen
/// Domains/Postfächern nicht unnötig viele einzelne Aufrufe braucht.
#[derive(Debug, Serialize)]
pub struct DomainOverviewEntry {
    pub id: Uuid,
    pub name: String,
    pub is_active: bool,
    pub user_count: i64,
    /// Konfiguriertes Domain-weites Limit (falls gesetzt) — aktuell nur
    /// zur Anzeige/Orientierung, keine technische Durchsetzung (im
    /// Gegensatz zu users.quota_bytes je Postfach, siehe
    /// config/dovecot/90-quota.conf.tera).
    pub quota_bytes: Option<i64>,
    /// `None`, wenn das Domain-Verzeichnis noch nicht existiert (keine
    /// einzige Mailbox hatte je ein IMAP-Login) oder `du` fehlschlug.
    pub storage_bytes: Option<i64>,
}

#[derive(Debug, FromRow)]
struct DomainOverviewRow {
    id: Uuid,
    name: String,
    is_active: bool,
    quota_bytes: Option<i64>,
    user_count: i64,
}

pub async fn domains_overview(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
) -> ApiResult<Json<Vec<DomainOverviewEntry>>> {
    // Nur super_admin — eine domänenübergreifende Übersicht ist per
    // Definition kein Ausschnitt, den ein domain_admin sehen darf.
    if !actor.can(Action::ManageSystemSettings, None) {
        return Err(ApiError::Forbidden);
    }

    let rows: Vec<DomainOverviewRow> = sqlx::query_as(
        r#"
        SELECT d.id, d.name, d.is_active, d.quota_bytes, COUNT(u.id) as user_count
        FROM domains d
        LEFT JOIN users u ON u.domain_id = d.id
        GROUP BY d.id
        ORDER BY d.name
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    let mail_base =
        std::env::var("HAVENMAIL_MAIL_DIR").unwrap_or_else(|_| "/var/mail/havenmail".to_string());

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let path = std::path::Path::new(&mail_base).join(&row.name);
        let storage_bytes = havenmail_core::mailbox_storage::usage_bytes(&path).await;
        out.push(DomainOverviewEntry {
            id: row.id,
            name: row.name,
            is_active: row.is_active,
            user_count: row.user_count,
            quota_bytes: row.quota_bytes,
            storage_bytes,
        });
    }

    Ok(Json(out))
}
