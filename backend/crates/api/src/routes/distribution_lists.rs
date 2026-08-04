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
pub struct DistributionList {
    pub id: Uuid,
    pub domain_id: Uuid,
    pub address: String,
    pub members: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateDistributionListRequest {
    pub address: String,
    pub members: Vec<String>,
}

pub async fn create_distribution_list(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(domain_id): Path<Uuid>,
    Json(req): Json<CreateDistributionListRequest>,
) -> ApiResult<Json<DistributionList>> {
    if !actor.can(Action::ManageDomain, Some(domain_id)) {
        return Err(ApiError::Forbidden);
    }
    if req.address.trim().is_empty() {
        return Err(ApiError::BadRequest("address erforderlich".to_string()));
    }

    let list: DistributionList = sqlx::query_as(
        r#"
        INSERT INTO distribution_lists (domain_id, address, members)
        VALUES ($1, $2, $3)
        RETURNING id, domain_id, address, members
        "#,
    )
    .bind(domain_id)
    .bind(req.address.trim().to_lowercase())
    .bind(&req.members)
    .fetch_one(&state.db)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
            ApiError::Conflict("Verteiler existiert bereits".to_string())
        }
        _ => ApiError::Internal(e),
    })?;

    Ok(Json(list))
}

pub async fn list_distribution_lists(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(domain_id): Path<Uuid>,
) -> ApiResult<Json<Vec<DistributionList>>> {
    if !actor.can(Action::ManageDomain, Some(domain_id)) {
        return Err(ApiError::Forbidden);
    }
    let lists: Vec<DistributionList> = sqlx::query_as(
        "SELECT id, domain_id, address, members FROM distribution_lists WHERE domain_id = $1 ORDER BY address",
    )
    .bind(domain_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(lists))
}

pub async fn delete_distribution_list(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(list_id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let domain_id: Option<Uuid> =
        sqlx::query_scalar("SELECT domain_id FROM distribution_lists WHERE id = $1")
            .bind(list_id)
            .fetch_optional(&state.db)
            .await?;
    let Some(domain_id) = domain_id else {
        return Err(ApiError::NotFound);
    };
    if !actor.can(Action::ManageDomain, Some(domain_id)) {
        return Err(ApiError::NotFound);
    }
    sqlx::query("DELETE FROM distribution_lists WHERE id = $1")
        .bind(list_id)
        .execute(&state.db)
        .await?;
    Ok(Json(serde_json::json!({ "status": "deleted" })))
}
