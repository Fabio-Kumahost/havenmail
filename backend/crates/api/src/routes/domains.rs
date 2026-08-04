use crate::auth_extractor::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
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
    pub created_at: chrono::DateTime<chrono::Utc>,
}

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
    Json(req): Json<CreateDomainRequest>,
) -> ApiResult<Json<Domain>> {
    if actor.role != Role::SuperAdmin {
        return Err(ApiError::Forbidden);
    }
    if req.name.trim().is_empty() || !req.name.contains('.') {
        return Err(ApiError::BadRequest("ungültiger Domain-Name".to_string()));
    }

    let domain: Domain = sqlx::query_as(
        r#"
        INSERT INTO domains (name, quota_bytes)
        VALUES ($1, $2)
        RETURNING id, name, is_active, catch_all_enabled, catch_all_target, quota_bytes, created_at
        "#,
    )
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

    Ok(Json(domain))
}

/// `super_admin` sieht alle Domains, `domain_admin`/`user` nur die eigene.
pub async fn list_domains(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
) -> ApiResult<Json<Vec<Domain>>> {
    let domains: Vec<Domain> = match actor.role {
        Role::SuperAdmin => {
            sqlx::query_as(
                "SELECT id, name, is_active, catch_all_enabled, catch_all_target, quota_bytes, created_at FROM domains ORDER BY name",
            )
            .fetch_all(&state.db)
            .await?
        }
        Role::DomainAdmin | Role::User => {
            let Some(domain_id) = actor.domain_id else {
                return Ok(Json(vec![]));
            };
            sqlx::query_as(
                "SELECT id, name, is_active, catch_all_enabled, catch_all_target, quota_bytes, created_at FROM domains WHERE id = $1",
            )
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
    let domain: Domain = sqlx::query_as(
        "SELECT id, name, is_active, catch_all_enabled, catch_all_target, quota_bytes, created_at FROM domains WHERE id = $1",
    )
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

    let current: Domain = sqlx::query_as(
        "SELECT id, name, is_active, catch_all_enabled, catch_all_target, quota_bytes, created_at FROM domains WHERE id = $1",
    )
    .bind(domain_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound)?;

    let is_active = req.is_active.unwrap_or(current.is_active);
    let catch_all_enabled = req.catch_all_enabled.unwrap_or(current.catch_all_enabled);
    let catch_all_target = req.catch_all_target.or(current.catch_all_target);
    let quota_bytes = req.quota_bytes.or(current.quota_bytes);

    let domain: Domain = sqlx::query_as(
        r#"
        UPDATE domains
        SET is_active = $2, catch_all_enabled = $3, catch_all_target = $4, quota_bytes = $5
        WHERE id = $1
        RETURNING id, name, is_active, catch_all_enabled, catch_all_target, quota_bytes, created_at
        "#,
    )
    .bind(domain_id)
    .bind(is_active)
    .bind(catch_all_enabled)
    .bind(&catch_all_target)
    .bind(quota_bytes)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(domain))
}

/// Löschen ist destruktiv (kaskadiert auf Benutzer/Aliase) — bewusst nur
/// `super_admin`, unabhängig vom sonst für ManageDomain reichenden Scope.
pub async fn delete_domain(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(domain_id): Path<Uuid>,
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
    Ok(Json(serde_json::json!({ "status": "deleted" })))
}
