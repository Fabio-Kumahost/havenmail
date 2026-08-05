//! Lesezugriff auf das unveränderliche Audit-Log (`havenmail_core::audit`).
//! `super_admin` sieht alle Einträge (optional nach `domain_id` gefiltert),
//! `domain_admin` nur Einträge der eigenen Domain (`Action::ViewAuditLog`
//! in `havenmail_core::rbac`). Kein Schreibzugriff über die API — Einträge
//! entstehen ausschließlich serverseitig über `audit_log::log`.

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
}

#[derive(Debug, FromRow, Serialize)]
pub struct AuditLogEntry {
    pub id: Uuid,
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

    let entries: Vec<AuditLogEntry> = match effective_domain_id {
        Some(domain_id) => {
            sqlx::query_as(
                r#"
                SELECT id, actor_id, action, target, domain_id, before, after, host(ip) as ip, created_at
                FROM audit_log
                WHERE domain_id = $1
                ORDER BY seq DESC
                LIMIT $2
                "#,
            )
            .bind(domain_id)
            .bind(limit)
            .fetch_all(&state.db)
            .await?
        }
        None => {
            sqlx::query_as(
                r#"
                SELECT id, actor_id, action, target, domain_id, before, after, host(ip) as ip, created_at
                FROM audit_log
                ORDER BY seq DESC
                LIMIT $1
                "#,
            )
            .bind(limit)
            .fetch_all(&state.db)
            .await?
        }
    };

    Ok(Json(entries))
}
