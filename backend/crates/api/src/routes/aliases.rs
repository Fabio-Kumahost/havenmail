use crate::auth_extractor::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    Json,
};
use havenmail_core::rbac::Action;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, FromRow, Serialize)]
pub struct Alias {
    pub id: Uuid,
    pub domain_id: Uuid,
    pub source: String,
    pub destinations: Vec<String>,
    pub is_active: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateAliasRequest {
    pub source: String,
    pub destinations: Vec<String>,
}

pub async fn create_alias(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(domain_id): Path<Uuid>,
    Json(req): Json<CreateAliasRequest>,
) -> ApiResult<Json<Alias>> {
    if !actor.can(Action::ManageDomain, Some(domain_id)) {
        return Err(ApiError::Forbidden);
    }
    if req.source.trim().is_empty() || req.destinations.is_empty() {
        return Err(ApiError::BadRequest(
            "source und mindestens ein Ziel in destinations erforderlich".to_string(),
        ));
    }

    let alias: Alias = sqlx::query_as(
        r#"
        INSERT INTO aliases (domain_id, source, destinations)
        VALUES ($1, $2, $3)
        RETURNING id, domain_id, source, destinations, is_active
        "#,
    )
    .bind(domain_id)
    .bind(req.source.trim().to_lowercase())
    .bind(&req.destinations)
    .fetch_one(&state.db)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
            ApiError::Conflict("Alias existiert bereits".to_string())
        }
        _ => ApiError::Internal(e),
    })?;

    Ok(Json(alias))
}

pub async fn list_aliases(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(domain_id): Path<Uuid>,
) -> ApiResult<Json<Vec<Alias>>> {
    if !actor.can(Action::ManageDomain, Some(domain_id)) {
        return Err(ApiError::Forbidden);
    }
    let aliases: Vec<Alias> = sqlx::query_as(
        "SELECT id, domain_id, source, destinations, is_active FROM aliases WHERE domain_id = $1 ORDER BY source",
    )
    .bind(domain_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(aliases))
}

pub async fn delete_alias(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(alias_id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let domain_id: Option<Uuid> = sqlx::query_scalar("SELECT domain_id FROM aliases WHERE id = $1")
        .bind(alias_id)
        .fetch_optional(&state.db)
        .await?;
    let Some(domain_id) = domain_id else {
        return Err(ApiError::NotFound);
    };
    if !actor.can(Action::ManageDomain, Some(domain_id)) {
        return Err(ApiError::NotFound);
    }
    sqlx::query("DELETE FROM aliases WHERE id = $1")
        .bind(alias_id)
        .execute(&state.db)
        .await?;
    Ok(Json(serde_json::json!({ "status": "deleted" })))
}
