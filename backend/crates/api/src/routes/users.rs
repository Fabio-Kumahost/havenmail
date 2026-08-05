use crate::auth_extractor::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use havenmail_core::auth::password;
use havenmail_core::rbac::{Action, Role};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, FromRow, Serialize)]
pub struct User {
    pub id: Uuid,
    pub domain_id: Uuid,
    pub local_part: String,
    pub role: String,
    pub quota_bytes: Option<i64>,
    pub is_active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub local_part: String,
    pub password: String,
    pub role: Option<String>,
    pub quota_bytes: Option<i64>,
}

pub async fn create_user(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(domain_id): Path<Uuid>,
    headers: HeaderMap,
    Json(req): Json<CreateUserRequest>,
) -> ApiResult<Json<User>> {
    if !actor.can(Action::ManageDomainUsers, Some(domain_id)) {
        return Err(ApiError::Forbidden);
    }
    if req.local_part.trim().is_empty() || req.password.len() < 12 {
        return Err(ApiError::BadRequest(
            "local_part erforderlich, Passwort muss mindestens 12 Zeichen haben".to_string(),
        ));
    }

    let requested_role = req.role.as_deref().unwrap_or("user");
    // Ein domain_admin darf keine super_admin-Konten anlegen (Schutz vor
    // Rechteausweitung über die eigene Domain hinaus).
    if requested_role == "super_admin" && actor.role != Role::SuperAdmin {
        return Err(ApiError::Forbidden);
    }
    if !["super_admin", "domain_admin", "user"].contains(&requested_role) {
        return Err(ApiError::BadRequest("ungültige Rolle".to_string()));
    }

    let password_hash =
        password::hash_password(&req.password).map_err(|e| ApiError::TokenIssue(e.to_string()))?;

    let user: User = sqlx::query_as(
        r#"
        INSERT INTO users (domain_id, local_part, password_hash, role, quota_bytes)
        VALUES ($1, $2, $3, $4::havenmail_user_role, $5)
        RETURNING id, domain_id, local_part, role::text as role, quota_bytes, is_active, created_at
        "#,
    )
    .bind(domain_id)
    .bind(req.local_part.trim().to_lowercase())
    .bind(&password_hash)
    .bind(requested_role)
    .bind(req.quota_bytes)
    .fetch_one(&state.db)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
            ApiError::Conflict("Postfach existiert bereits".to_string())
        }
        _ => ApiError::Internal(e),
    })?;

    crate::audit_log::log(
        &state,
        &actor,
        &headers,
        "user.create",
        &user.id.to_string(),
        Some(user.domain_id),
        None,
        serde_json::to_value(&user).ok(),
    )
    .await;

    Ok(Json(user))
}

pub async fn list_users(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(domain_id): Path<Uuid>,
) -> ApiResult<Json<Vec<User>>> {
    if !actor.can(Action::ManageDomainUsers, Some(domain_id)) {
        return Err(ApiError::Forbidden);
    }
    let users: Vec<User> = sqlx::query_as(
        "SELECT id, domain_id, local_part, role::text as role, quota_bytes, is_active, created_at FROM users WHERE domain_id = $1 ORDER BY local_part",
    )
    .bind(domain_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(users))
}

async fn fetch_user_or_404(pool: &sqlx::PgPool, user_id: Uuid) -> ApiResult<User> {
    sqlx::query_as(
        "SELECT id, domain_id, local_part, role::text as role, quota_bytes, is_active, created_at FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .ok_or(ApiError::NotFound)
}

pub async fn get_user(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(user_id): Path<Uuid>,
) -> ApiResult<Json<User>> {
    let user = fetch_user_or_404(&state.db, user_id).await?;
    let allowed = actor.owns(user.id) || actor.can(Action::ManageDomainUsers, Some(user.domain_id));
    if !allowed {
        return Err(ApiError::NotFound);
    }
    Ok(Json(user))
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    pub password: Option<String>,
    pub is_active: Option<bool>,
    pub quota_bytes: Option<i64>,
}

pub async fn update_user(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(user_id): Path<Uuid>,
    headers: HeaderMap,
    Json(req): Json<UpdateUserRequest>,
) -> ApiResult<Json<User>> {
    let current = fetch_user_or_404(&state.db, user_id).await?;
    let is_self = actor.owns(current.id);
    let is_domain_manager = actor.can(Action::ManageDomainUsers, Some(current.domain_id));
    if !is_self && !is_domain_manager {
        return Err(ApiError::NotFound);
    }
    // Nur Domain-Verwalter dürfen Aktivierungsstatus/Quota ändern, nicht der
    // Nutzer selbst (verhindert Selbst-Reaktivierung eines gesperrten Kontos).
    if (req.is_active.is_some() || req.quota_bytes.is_some()) && !is_domain_manager {
        return Err(ApiError::Forbidden);
    }
    if let Some(ref pw) = req.password {
        if pw.len() < 12 {
            return Err(ApiError::BadRequest(
                "Passwort muss mindestens 12 Zeichen haben".to_string(),
            ));
        }
    }

    let new_password_hash = match &req.password {
        Some(pw) => {
            Some(password::hash_password(pw).map_err(|e| ApiError::TokenIssue(e.to_string()))?)
        }
        None => None,
    };
    let is_active = req.is_active.unwrap_or(current.is_active);
    let quota_bytes = req.quota_bytes.or(current.quota_bytes);

    let user: User = if let Some(hash) = new_password_hash {
        sqlx::query_as(
            r#"
            UPDATE users SET password_hash = $2, is_active = $3, quota_bytes = $4
            WHERE id = $1
            RETURNING id, domain_id, local_part, role::text as role, quota_bytes, is_active, created_at
            "#,
        )
        .bind(user_id)
        .bind(hash)
        .bind(is_active)
        .bind(quota_bytes)
        .fetch_one(&state.db)
        .await?
    } else {
        sqlx::query_as(
            r#"
            UPDATE users SET is_active = $2, quota_bytes = $3
            WHERE id = $1
            RETURNING id, domain_id, local_part, role::text as role, quota_bytes, is_active, created_at
            "#,
        )
        .bind(user_id)
        .bind(is_active)
        .bind(quota_bytes)
        .fetch_one(&state.db)
        .await?
    };

    // Passwort-Änderung wird im Aktionsnamen vermerkt, der Wert selbst nie
    // (weder Klartext noch Hash landen im Audit-Log, das nur `before`/`after`
    // aus den obigen User-Structs übernimmt — die enthalten kein Passwortfeld).
    let action = if req.password.is_some() {
        "user.update_with_password_change"
    } else {
        "user.update"
    };
    crate::audit_log::log(
        &state,
        &actor,
        &headers,
        action,
        &user_id.to_string(),
        Some(current.domain_id),
        serde_json::to_value(&current).ok(),
        serde_json::to_value(&user).ok(),
    )
    .await;

    Ok(Json(user))
}

#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Debug, FromRow)]
struct PasswordHashRow {
    password_hash: String,
}

/// Selbstbedienungs-Passwortänderung — anders als `update_user` (das ein
/// Domain-/Systemadmin auch ohne Kenntnis des Altpassworts nutzen kann, um
/// ein fremdes Konto zurückzusetzen) verlangt dieser Endpunkt das aktuelle
/// Passwort. Aus dem JWT über `AuthUser` aufgelöst statt über einen
/// `:user_id`-Pfadparameter, da das Frontend die eigene User-ID nicht kennt
/// (kein Client-seitiges JWT-Decoding, siehe api.ts).
pub async fn change_own_password(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    headers: HeaderMap,
    Json(req): Json<ChangePasswordRequest>,
) -> ApiResult<Json<User>> {
    if !actor.can(Action::ManageOwnAccount, None) {
        return Err(ApiError::Forbidden);
    }
    if req.new_password.len() < 12 {
        return Err(ApiError::BadRequest(
            "neues Passwort muss mindestens 12 Zeichen haben".to_string(),
        ));
    }

    let row: PasswordHashRow =
        sqlx::query_as("SELECT password_hash FROM users WHERE id = $1")
            .bind(actor.user_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or(ApiError::NotFound)?;

    if !password::verify_password(&req.current_password, &row.password_hash) {
        return Err(ApiError::BadRequest(
            "aktuelles Passwort ist falsch".to_string(),
        ));
    }

    let new_hash = password::hash_password(&req.new_password)
        .map_err(|e| ApiError::TokenIssue(e.to_string()))?;

    let user: User = sqlx::query_as(
        r#"
        UPDATE users SET password_hash = $2
        WHERE id = $1
        RETURNING id, domain_id, local_part, role::text as role, quota_bytes, is_active, created_at
        "#,
    )
    .bind(actor.user_id)
    .bind(new_hash)
    .fetch_one(&state.db)
    .await?;

    // Wie bei update_user: nie das Passwort selbst loggen, nur dass es
    // sich geändert hat.
    crate::audit_log::log(
        &state,
        &actor,
        &headers,
        "user.change_own_password",
        &actor.user_id.to_string(),
        Some(user.domain_id),
        None,
        None,
    )
    .await;

    Ok(Json(user))
}

pub async fn delete_user(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(user_id): Path<Uuid>,
    headers: HeaderMap,
) -> ApiResult<Json<serde_json::Value>> {
    let current = fetch_user_or_404(&state.db, user_id).await?;
    if !actor.can(Action::ManageDomainUsers, Some(current.domain_id)) {
        return Err(ApiError::NotFound);
    }
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&state.db)
        .await?;

    crate::audit_log::log(
        &state,
        &actor,
        &headers,
        "user.delete",
        &user_id.to_string(),
        Some(current.domain_id),
        serde_json::to_value(&current).ok(),
        None,
    )
    .await;

    Ok(Json(serde_json::json!({ "status": "deleted" })))
}
