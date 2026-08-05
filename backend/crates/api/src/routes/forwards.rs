//! Weiterleitungen mit Schutz vor Mail-Loops (siehe docs/architecture.md,
//! Bedrohungsanalyse: "Mail-Loops bei Weiterleitungen").
//!
//! Die Prüfung erfolgt vor dem Aktivieren einer Weiterleitung durch
//! Verfolgen der Ziel-Kette bis zu einer Tiefe von 25 Hops: Landet die Kette
//! wieder beim Absender, wird die Weiterleitung abgelehnt. Das deckt direkte
//! und mehrstufige Zyklen zwischen lokalen Postfächern ab; externe Ziele
//! (fremde Domains) beenden die Kette, da sie nicht auflösbar sind.

use crate::auth_extractor::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use havenmail_core::rbac::Action;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgPool};
use std::collections::HashSet;
use uuid::Uuid;

const MAX_LOOP_CHECK_DEPTH: usize = 25;

#[derive(Debug, FromRow, Serialize)]
pub struct Forward {
    pub id: Uuid,
    pub user_id: Uuid,
    pub target_address: String,
    pub keep_copy: bool,
    pub is_active: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateForwardRequest {
    pub target_address: String,
    #[serde(default = "default_true")]
    pub keep_copy: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, FromRow)]
struct UserEmailRow {
    domain_id: Uuid,
    email: String,
}

async fn fetch_user_email(pool: &PgPool, user_id: Uuid) -> ApiResult<UserEmailRow> {
    sqlx::query_as(
        r#"
        SELECT u.domain_id, u.local_part || '@' || d.name as email
        FROM users u JOIN domains d ON d.id = u.domain_id
        WHERE u.id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .ok_or(ApiError::NotFound)
}

/// Folgt der Weiterleitungskette ab `start_email` und prüft, ob `origin_email`
/// darin wieder auftaucht (= Schleife). Bricht bei nicht-lokalen Zieladressen,
/// bereits besuchten Adressen (fremder Zyklus) oder Tiefenlimit ab.
async fn creates_loop(pool: &PgPool, origin_email: &str, start_email: &str) -> ApiResult<bool> {
    let mut current = start_email.to_string();
    let mut visited: HashSet<String> = HashSet::new();

    for _ in 0..MAX_LOOP_CHECK_DEPTH {
        if current.eq_ignore_ascii_case(origin_email) {
            return Ok(true);
        }
        if !visited.insert(current.clone()) {
            break; // Zyklus, der origin_email nicht enthält -> nicht unser Problem hier
        }

        let user_id: Option<Uuid> = sqlx::query_scalar(
            r#"
            SELECT u.id FROM users u JOIN domains d ON d.id = u.domain_id
            WHERE u.local_part || '@' || d.name = $1 AND u.is_active AND d.is_active
            "#,
        )
        .bind(&current)
        .fetch_optional(pool)
        .await?;

        let Some(user_id) = user_id else {
            break; // kein lokales Postfach -> Kette endet extern
        };

        let next: Option<String> = sqlx::query_scalar(
            "SELECT target_address FROM forwards WHERE user_id = $1 AND is_active LIMIT 1",
        )
        .bind(user_id)
        .fetch_optional(pool)
        .await?;

        match next {
            Some(next_target) => current = next_target,
            None => break,
        }
    }

    Ok(false)
}

fn fingerprint(target_address: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(target_address.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub async fn create_forward(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(user_id): Path<Uuid>,
    headers: HeaderMap,
    Json(req): Json<CreateForwardRequest>,
) -> ApiResult<Json<Forward>> {
    let owner = fetch_user_email(&state.db, user_id).await?;
    let allowed =
        actor.owns(user_id) || actor.can(Action::ManageDomainUsers, Some(owner.domain_id));
    if !allowed {
        return Err(ApiError::NotFound);
    }

    let target = req.target_address.trim().to_lowercase();
    if target.is_empty() || !target.contains('@') {
        return Err(ApiError::BadRequest("ungültige Zieladresse".to_string()));
    }
    if target.eq_ignore_ascii_case(&owner.email) {
        return Err(ApiError::BadRequest(
            "Weiterleitung auf die eigene Adresse ist nicht zulässig".to_string(),
        ));
    }
    if creates_loop(&state.db, &owner.email, &target).await? {
        return Err(ApiError::Conflict(
            "Diese Weiterleitung würde eine Mail-Schleife erzeugen".to_string(),
        ));
    }

    let forward: Forward = sqlx::query_as(
        r#"
        INSERT INTO forwards (user_id, target_address, keep_copy, loop_guard_hash)
        VALUES ($1, $2, $3, $4)
        RETURNING id, user_id, target_address, keep_copy, is_active
        "#,
    )
    .bind(user_id)
    .bind(&target)
    .bind(req.keep_copy)
    .bind(fingerprint(&target))
    .fetch_one(&state.db)
    .await?;

    crate::audit_log::log(
        &state,
        &actor,
        &headers,
        "forward.create",
        &forward.id.to_string(),
        Some(owner.domain_id),
        None,
        serde_json::to_value(&forward).ok(),
    )
    .await;

    Ok(Json(forward))
}

pub async fn list_forwards(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(user_id): Path<Uuid>,
) -> ApiResult<Json<Vec<Forward>>> {
    let owner = fetch_user_email(&state.db, user_id).await?;
    let allowed =
        actor.owns(user_id) || actor.can(Action::ManageDomainUsers, Some(owner.domain_id));
    if !allowed {
        return Err(ApiError::NotFound);
    }
    let forwards: Vec<Forward> = sqlx::query_as(
        "SELECT id, user_id, target_address, keep_copy, is_active FROM forwards WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(forwards))
}

pub async fn delete_forward(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(forward_id): Path<Uuid>,
    headers: HeaderMap,
) -> ApiResult<Json<serde_json::Value>> {
    let row: Option<(Uuid, Uuid, String)> = sqlx::query_as(
        r#"
        SELECT f.user_id, u.domain_id, f.target_address FROM forwards f
        JOIN users u ON u.id = f.user_id
        WHERE f.id = $1
        "#,
    )
    .bind(forward_id)
    .fetch_optional(&state.db)
    .await?;

    let Some((owner_user_id, domain_id, target_address)) = row else {
        return Err(ApiError::NotFound);
    };
    let allowed =
        actor.owns(owner_user_id) || actor.can(Action::ManageDomainUsers, Some(domain_id));
    if !allowed {
        return Err(ApiError::NotFound);
    }

    sqlx::query("DELETE FROM forwards WHERE id = $1")
        .bind(forward_id)
        .execute(&state.db)
        .await?;

    crate::audit_log::log(
        &state,
        &actor,
        &headers,
        "forward.delete",
        &forward_id.to_string(),
        Some(domain_id),
        Some(serde_json::json!({ "user_id": owner_user_id, "target_address": target_address })),
        None,
    )
    .await;

    Ok(Json(serde_json::json!({ "status": "deleted" })))
}
