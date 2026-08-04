//! Axum-Extractor, der ein gültiges JWT aus dem `Authorization: Bearer`-Header
//! in einen `havenmail_core::rbac::Actor` übersetzt. Jeder geschützte Handler
//! nimmt `AuthUser` als Parameter entgegen — es gibt keinen Weg, an dieser
//! Prüfung vorbeizukommen, ohne den Parameter wegzulassen (dann ist der
//! Endpunkt bewusst öffentlich, z. B. `/auth/login`).

use crate::error::ApiError;
use crate::state::AppState;
use async_trait::async_trait;
use axum::{
    extract::FromRequestParts,
    http::{header, request::Parts},
};
use havenmail_core::rbac::Actor;

pub struct AuthUser(pub Actor);

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

        let token = header_value
            .strip_prefix("Bearer ")
            .ok_or(ApiError::Unauthorized)?;

        let claims = state
            .jwt
            .verify(token)
            .map_err(|_| ApiError::Unauthorized)?;

        Ok(AuthUser(Actor {
            user_id: claims.sub,
            role: claims.role,
            domain_id: claims.domain_id,
        }))
    }
}
