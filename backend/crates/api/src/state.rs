use havenmail_core::auth::jwt::JwtIssuer;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub jwt: Arc<JwtIssuer>,
}
