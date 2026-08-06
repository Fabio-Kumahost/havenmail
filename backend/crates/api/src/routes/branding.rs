//! White-Label-Branding (Produktname/Logo/Akzentfarbe) fürs Admin-Panel.
//!
//! `GET` ist bewusst ÖFFENTLICH (kein `AuthUser`) — die Login-Seite selbst
//! muss die Branding-Werte schon vor jeder Authentifizierung anzeigen
//! können. Enthält keine sensiblen Daten (Produktname/Logo-URL/Farbe),
//! also unproblematisch. `PATCH` bleibt wie alle anderen Systemeinstellungen
//! `super_admin`-only (`Action::ManageSystemSettings`).

use crate::auth_extractor::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::{extract::State, http::HeaderMap, Json};
use havenmail_core::rbac::Action;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct BrandingSettings {
    pub product_name: String,
    pub logo_url: Option<String>,
    pub accent_color: Option<String>,
}

const SELECT_COLUMNS: &str = "product_name, logo_url, accent_color";

async fn fetch_settings(pool: &sqlx::PgPool) -> ApiResult<BrandingSettings> {
    Ok(sqlx::query_as(&format!(
        "SELECT {SELECT_COLUMNS} FROM branding_settings WHERE id = 1"
    ))
    .fetch_one(pool)
    .await?)
}

pub async fn get_branding(State(state): State<AppState>) -> ApiResult<Json<BrandingSettings>> {
    Ok(Json(fetch_settings(&state.db).await?))
}

#[derive(Debug, Deserialize)]
pub struct UpdateBrandingRequest {
    pub product_name: String,
    #[serde(default)]
    pub logo_url: Option<String>,
    #[serde(default)]
    pub accent_color: Option<String>,
}

/// Sehr grobe Plausibilitätsprüfung — kein vollständiger CSS-Color-Parser
/// nötig, nur ein Schutz gegen offensichtlich falsche Eingaben (leere
/// Werte, versehentlich eingefügtes HTML/JS-Fragment). Der Wert landet
/// unverändert als CSS-Custom-Property (`--accent`), niemals in innerHTML
/// o. Ä. eingesetzt — kein XSS-Risiko, nur UX-Schutz vor Tippfehlern.
fn is_plausible_css_color(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() || value.len() > 40 {
        return false;
    }
    value.chars().all(|c| {
        c.is_ascii_alphanumeric() || matches!(c, '#' | '(' | ')' | ',' | '.' | '%' | ' ' | '-')
    })
}

pub async fn update_branding(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    headers: HeaderMap,
    Json(req): Json<UpdateBrandingRequest>,
) -> ApiResult<Json<BrandingSettings>> {
    if !actor.can(Action::ManageSystemSettings, None) {
        return Err(ApiError::Forbidden);
    }
    let product_name = req.product_name.trim();
    if product_name.is_empty() || product_name.len() > 60 {
        return Err(ApiError::BadRequest(
            "Produktname muss 1–60 Zeichen lang sein".to_string(),
        ));
    }
    if let Some(url) = &req.logo_url {
        if !url.trim().is_empty() && !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err(ApiError::BadRequest(
                "Logo-URL muss mit http:// oder https:// beginnen".to_string(),
            ));
        }
    }
    if let Some(color) = &req.accent_color {
        if !color.trim().is_empty() && !is_plausible_css_color(color) {
            return Err(ApiError::BadRequest(
                "Akzentfarbe sieht nicht wie ein gültiger CSS-Farbwert aus".to_string(),
            ));
        }
    }

    let logo_url = req.logo_url.filter(|s| !s.trim().is_empty());
    let accent_color = req.accent_color.filter(|s| !s.trim().is_empty());

    let settings: BrandingSettings = sqlx::query_as(&format!(
        r#"
        UPDATE branding_settings
        SET product_name = $1, logo_url = $2, accent_color = $3,
            updated_at = now(), updated_by = $4
        WHERE id = 1
        RETURNING {SELECT_COLUMNS}
        "#
    ))
    .bind(product_name)
    .bind(&logo_url)
    .bind(&accent_color)
    .bind(actor.user_id)
    .fetch_one(&state.db)
    .await?;

    crate::audit_log::log(
        &state,
        &actor,
        &headers,
        "branding.update",
        "branding",
        None,
        None,
        serde_json::to_value(&settings).ok(),
    )
    .await;

    Ok(Json(settings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_hex_and_rgb_colors() {
        assert!(is_plausible_css_color("#4f46e5"));
        assert!(is_plausible_css_color("rgb(79, 70, 229)"));
        assert!(is_plausible_css_color("hsl(243, 75%, 59%)"));
    }

    #[test]
    fn rejects_empty_or_html_looking_input() {
        assert!(!is_plausible_css_color(""));
        assert!(!is_plausible_css_color("<script>alert(1)</script>"));
        assert!(!is_plausible_css_color(&"a".repeat(41)));
    }
}
