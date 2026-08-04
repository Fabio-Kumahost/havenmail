//! Einheitliche Fehlerbehandlung für die REST-API.
//!
//! Fehlermeldungen sind bewusst generisch gehalten (kein Leaken von
//! Datenbankdetails, keine unterscheidbaren Meldungen für "existiert nicht"
//! vs. "keine Berechtigung" bei fremden Ressourcen — Schutz vor Enumeration,
//! siehe docs/architecture.md, Bedrohungsanalyse).

use axum::{http::StatusCode, response::IntoResponse, response::Response, Json};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("nicht authentifiziert")]
    Unauthorized,
    #[error("keine Berechtigung")]
    Forbidden,
    #[error("Ressource nicht gefunden")]
    NotFound,
    #[error("ungültige Eingabe: {0}")]
    BadRequest(String),
    #[error("Konflikt: {0}")]
    Conflict(String),
    #[error("interner Fehler")]
    Internal(#[from] sqlx::Error),
    #[error("interner Fehler (Token)")]
    TokenIssue(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, self.to_string()),
            ApiError::Forbidden => (StatusCode::FORBIDDEN, self.to_string()),
            ApiError::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            ApiError::BadRequest(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            ApiError::Conflict(_) => (StatusCode::CONFLICT, self.to_string()),
            ApiError::Internal(err) => {
                tracing::error!(%err, "interner Fehler");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "interner Fehler".to_string(),
                )
            }
            ApiError::TokenIssue(err) => {
                tracing::error!(%err, "Token-Ausstellung fehlgeschlagen");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "interner Fehler".to_string(),
                )
            }
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
