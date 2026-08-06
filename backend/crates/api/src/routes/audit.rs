//! Lesezugriff auf das unveränderliche Audit-Log (`havenmail_core::audit`).
//! `super_admin` sieht alle Einträge (optional nach `domain_id`/`action`/
//! Zeitraum gefiltert), `domain_admin` nur Einträge der eigenen Domain
//! (`Action::ViewAuditLog` in `havenmail_core::rbac`). Kein Schreibzugriff
//! über die API — Einträge entstehen ausschließlich serverseitig über
//! `audit_log::log`.
//!
//! Pagination per Cursor (`before_seq`), nicht per `OFFSET`: `seq` ist
//! strikt monoton in Einfüge-Reihenfolge (siehe
//! `0003_audit_log_seq.sql`) und ändert sich nie rückwirkend — ein
//! Cursor bleibt stabil auch wenn zwischen zwei "mehr laden"-Klicks neue
//! Einträge dazukommen, ein `OFFSET` würde dagegen Einträge doppelt zeigen
//! oder überspringen.

use crate::auth_extractor::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::extract::{Query, State};
use axum::Json;
use havenmail_core::rbac::{Action, Role};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct AuditLogQuery {
    domain_id: Option<Uuid>,
    /// Maximal 200 Einträge pro Abfrage, Standard 50 — kein unbegrenzter
    /// Full-Table-Scan über eine womöglich langjährig gewachsene Kette.
    limit: Option<i64>,
    /// Cursor fürs Nachladen: nur Einträge mit `seq` kleiner als dieser
    /// Wert (aus dem `seq`-Feld des letzten Eintrags der vorherigen Seite).
    before_seq: Option<i64>,
    /// Exakter Aktionsname (z. B. "user.create", "notify.alert") — keine
    /// Teilstring-/Wildcard-Suche, das Frontend bietet eine feste Liste
    /// bekannter Aktionsnamen zur Auswahl an.
    action: Option<String>,
    since: Option<chrono::DateTime<chrono::Utc>>,
    until: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, FromRow, Serialize)]
pub struct AuditLogEntry {
    pub id: Uuid,
    /// Cursor-Wert für `before_seq` der nächsten Seite.
    pub seq: i64,
    pub actor_id: Option<Uuid>,
    pub action: String,
    pub target: String,
    pub domain_id: Option<Uuid>,
    pub before: Option<serde_json::Value>,
    pub after: Option<serde_json::Value>,
    pub ip: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn list_audit_log(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Query(query): Query<AuditLogQuery>,
) -> ApiResult<Json<Vec<AuditLogEntry>>> {
    let limit = query.limit.unwrap_or(50).clamp(1, 200);

    // Scope bestimmen: super_admin darf frei filtern (oder alles sehen),
    // domain_admin ist zwingend auf die eigene Domain beschränkt — ein
    // fremdes domain_id im Query-Parameter wird ignoriert, nicht als Fehler
    // gemeldet (kein Enumerations-Signal, siehe error.rs-Konvention).
    let effective_domain_id = match actor.role {
        Role::SuperAdmin => query.domain_id,
        Role::DomainAdmin => {
            let Some(own_domain) = actor.domain_id else {
                return Ok(Json(vec![]));
            };
            if !actor.can(Action::ViewAuditLog, Some(own_domain)) {
                return Err(ApiError::Forbidden);
            }
            Some(own_domain)
        }
        Role::User => return Err(ApiError::Forbidden),
    };

    let mut qb = sqlx::QueryBuilder::new(
        "SELECT id, seq, actor_id, action, target, domain_id, before, after, host(ip) as ip, created_at \
         FROM audit_log WHERE 1=1",
    );
    if let Some(domain_id) = effective_domain_id {
        qb.push(" AND domain_id = ").push_bind(domain_id);
    }
    if let Some(before_seq) = query.before_seq {
        qb.push(" AND seq < ").push_bind(before_seq);
    }
    if let Some(action) = &query.action {
        qb.push(" AND action = ").push_bind(action);
    }
    if let Some(since) = query.since {
        qb.push(" AND created_at >= ").push_bind(since);
    }
    if let Some(until) = query.until {
        qb.push(" AND created_at <= ").push_bind(until);
    }
    qb.push(" ORDER BY seq DESC LIMIT ").push_bind(limit);

    let entries: Vec<AuditLogEntry> = qb.build_query_as().fetch_all(&state.db).await?;
    Ok(Json(entries))
}

/// Bekannte Aktionsnamen fürs Filter-Dropdown im Frontend — statt bei
/// jeder neuen Aktion irgendwo eine Liste von Hand zu pflegen, wird sie
/// aus den tatsächlich vorhandenen Einträgen abgeleitet (`DISTINCT
/// action`). Auf denselben Scope beschränkt wie `list_audit_log`, damit
/// ein domain_admin keine Aktionsnamen sieht, die nur in fremden Domains
/// je vorkamen (geringes Informationsleck sonst, z. B. "backup.trigger"
/// verrät, dass Backups existieren).
pub async fn list_audit_actions(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
) -> ApiResult<Json<Vec<String>>> {
    let effective_domain_id = match actor.role {
        Role::SuperAdmin => None,
        Role::DomainAdmin => {
            let Some(own_domain) = actor.domain_id else {
                return Ok(Json(vec![]));
            };
            if !actor.can(Action::ViewAuditLog, Some(own_domain)) {
                return Err(ApiError::Forbidden);
            }
            Some(own_domain)
        }
        Role::User => return Err(ApiError::Forbidden),
    };

    let actions: Vec<String> = match effective_domain_id {
        Some(domain_id) => {
            sqlx::query_scalar(
                "SELECT DISTINCT action FROM audit_log WHERE domain_id = $1 ORDER BY action",
            )
            .bind(domain_id)
            .fetch_all(&state.db)
            .await?
        }
        None => {
            sqlx::query_scalar("SELECT DISTINCT action FROM audit_log ORDER BY action")
                .fetch_all(&state.db)
                .await?
        }
    };
    Ok(Json(actions))
}
