//! Axum-Extractor, der einen `Authorization: Bearer`-Header in einen
//! `havenmail_core::rbac::Actor` übersetzt. Jeder geschützte Handler nimmt
//! `AuthUser` als Parameter entgegen — es gibt keinen Weg, an dieser Prüfung
//! vorbeizukommen, ohne den Parameter wegzulassen (dann ist der Endpunkt
//! bewusst öffentlich, z. B. `/auth/login`).
//!
//! Zwei Token-Arten werden akzeptiert: ein JWT (interaktive Anmeldung, siehe
//! `routes/auth.rs`) oder ein API-Key mit `hvm_`-Präfix (siehe
//! `routes/api_tokens.rs`) — Refresh-Tokens haben zwar dasselbe Präfix, aber
//! werden nie als Bearer-Header vorgelegt (nur im Body von `/auth/refresh`),
//! also keine Verwechslungsgefahr. Der Präfix-Check entscheidet den Pfad,
//! damit der normale (weit häufigere) JWT-Fall keine zusätzliche DB-Anfrage
//! braucht.

use crate::error::ApiError;
use crate::state::AppState;
use async_trait::async_trait;
use axum::{
    extract::FromRequestParts,
    http::{header, request::Parts},
};
use havenmail_core::auth::token;
use havenmail_core::rbac::{Actor, Role};
use sqlx::FromRow;

pub struct AuthUser(pub Actor);

#[derive(Debug, FromRow)]
struct ApiTokenActorRow {
    user_id: uuid::Uuid,
    role: String,
    domain_id: uuid::Uuid,
    is_active: bool,
}

#[async_trait]
impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header_value = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or(ApiError::Unauthorized)?;

        let token_str = header_value
            .strip_prefix("Bearer ")
            .ok_or(ApiError::Unauthorized)?;

        if token_str.starts_with("hvm_") {
            return Self::from_api_token(token_str, state).await;
        }

        let claims = state
            .jwt
            .verify(token_str)
            .map_err(|_| ApiError::Unauthorized)?;

        Ok(AuthUser(Actor {
            user_id: claims.sub,
            role: claims.role,
            domain_id: claims.domain_id,
            session_id: claims.session_id,
        }))
    }
}

impl AuthUser {
    async fn from_api_token(token_str: &str, state: &AppState) -> Result<Self, ApiError> {
        let hash = token::hash_token(token_str);
        let row: Option<ApiTokenActorRow> = sqlx::query_as(
            r#"
            SELECT u.id as user_id, u.role::text as role, u.domain_id, u.is_active
            FROM api_tokens t
            JOIN users u ON u.id = t.user_id
            WHERE t.token_hash = $1
              AND t.revoked_at IS NULL
              AND (t.expires_at IS NULL OR t.expires_at > now())
            "#,
        )
        .bind(&hash)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| ApiError::Unauthorized)?;

        let Some(row) = row else {
            return Err(ApiError::Unauthorized);
        };
        if !row.is_active {
            return Err(ApiError::Unauthorized);
        }

        let role = match row.role.as_str() {
            "super_admin" => Role::SuperAdmin,
            "domain_admin" => Role::DomainAdmin,
            _ => Role::User,
        };
        let domain_id = if role == Role::SuperAdmin {
            None
        } else {
            Some(row.domain_id)
        };

        Ok(AuthUser(Actor {
            user_id: row.user_id,
            role,
            domain_id,
            // API-Keys sind keine Browser-Sitzung — nil statt einer echten
            // sessions.id, damit sie in der Sitzungsverwaltung (die eigene
            // Browser-Session markiert) niemals fälschlich als "aktuelle
            // Sitzung" erscheinen.
            session_id: uuid::Uuid::nil(),
        }))
    }
}
