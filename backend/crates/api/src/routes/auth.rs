//! Login/Refresh/Logout. Öffentlich erreichbar (kein `AuthUser`-Extractor
//! bei Login), da hier die Authentifizierung erst stattfindet.

use crate::auth_extractor::AuthUser;
use crate::client_ip;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::{extract::State, http::HeaderMap, Json};
use havenmail_core::auth::{jwt::ACCESS_TOKEN_TTL_SECONDS, password, token, totp};
use havenmail_core::rbac::Role;
use havenmail_core::secrets_crypto;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Maximale Lebensdauer eines Refresh-Tokens, bevor eine erneute
/// Passwort-Anmeldung nötig ist (siehe docs/architecture.md, Sicherheitsmodell).
const REFRESH_TOKEN_MAX_AGE_DAYS: i64 = 30;

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
    /// Nur nötig, wenn das Konto TOTP aktiviert hat (siehe `routes/totp.rs`).
    /// Fehlt er bei einem 2FA-Konto oder ist er falsch, liefert `login`
    /// `{"totp_required": true}` statt Tokens — die Passwortprüfung ist zu
    /// diesem Zeitpunkt bereits erfolgreich, also kein Enumerations-Risiko
    /// durch die unterschiedliche Antwortform an dieser Stelle.
    #[serde(default)]
    pub totp_code: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: &'static str,
    pub expires_in: i64,
}

#[derive(Debug, FromRow)]
struct UserAuthRow {
    id: Uuid,
    password_hash: String,
    role: String,
    domain_id: Uuid,
    is_active: bool,
    totp_secret_enc: Option<Vec<u8>>,
}

/// Konstant gehaltener Dummy-Hash für den Fall "Benutzer nicht gefunden" —
/// so unterscheidet sich die Antwortzeit für "falsches Passwort" und
/// "Account existiert nicht" nicht durch das Fehlen der Argon2-Berechnung
/// (Schutz vor Benutzer-Enumeration über Timing, siehe Bedrohungsanalyse).
fn dummy_hash() -> String {
    password::hash_password("havenmail-dummy-timing-equalizer").expect("statischer Dummy-Hash")
}

pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let ip = client_ip::extract(&headers);
    if state.login_rate_limiter.is_blocked(ip) {
        return Err(ApiError::TooManyRequests);
    }

    let row: Option<UserAuthRow> = sqlx::query_as(
        r#"
        SELECT u.id, u.password_hash, u.role::text as role, u.domain_id, u.is_active, u.totp_secret_enc
        FROM users u
        JOIN domains d ON d.id = u.domain_id
        WHERE u.local_part || '@' || d.name = $1 AND d.is_active
        "#,
    )
    .bind(&req.email)
    .fetch_optional(&state.db)
    .await?;

    let (user_id, stored_hash, role, domain_id, is_active, totp_secret_enc) = match &row {
        Some(r) => (
            r.id,
            r.password_hash.clone(),
            r.role.clone(),
            r.domain_id,
            r.is_active,
            r.totp_secret_enc.clone(),
        ),
        None => (
            Uuid::nil(),
            dummy_hash(),
            "user".to_string(),
            Uuid::nil(),
            false,
            None,
        ),
    };

    let password_ok = password::verify_password(&req.password, &stored_hash);
    if row.is_none() || !is_active || !password_ok {
        state.login_rate_limiter.record_failure(ip);
        return Err(ApiError::Unauthorized);
    }
    state.login_rate_limiter.record_success(ip);

    if let Some(encrypted_secret) = totp_secret_enc {
        let secret = secrets_crypto::decrypt(&state.secrets_key, &encrypted_secret)
            .map_err(|e| ApiError::TokenIssue(e.to_string()))?;
        let code_ok = req
            .totp_code
            .as_deref()
            .map(|code| totp::verify_code(&secret, code).unwrap_or(false))
            .unwrap_or(false);
        if !code_ok {
            // Absichtlich kein Rate-Limit-Fehlschlag hier: das Passwort war
            // bereits korrekt, ein falscher/fehlender TOTP-Code ist kein
            // Enumerations- oder Brute-Force-Signal auf das Passwort selbst.
            // Der Login-Rate-Limiter deckt weiterhin Passwort-Rateraten ab;
            // TOTP-Codes haben ohnehin nur 30s Gültigkeit.
            return Ok(Json(serde_json::json!({ "totp_required": true })));
        }
    }

    let role = parse_role(&role);
    let tokens = issue_token_pair(&state, user_id, role, domain_id).await?;
    Ok(Json(
        serde_json::to_value(tokens.0).expect("TokenResponse ist immer serialisierbar"),
    ))
}

#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Debug, FromRow)]
struct SessionRow {
    id: Uuid,
    user_id: Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn refresh(
    State(state): State<AppState>,
    Json(req): Json<RefreshRequest>,
) -> ApiResult<Json<TokenResponse>> {
    let hash = token::hash_token(&req.refresh_token);

    let session: Option<SessionRow> = sqlx::query_as(
        r#"
        SELECT id, user_id, created_at
        FROM sessions
        WHERE refresh_token_hash = $1 AND revoked_at IS NULL
        "#,
    )
    .bind(&hash)
    .fetch_optional(&state.db)
    .await?;

    let Some(session) = session else {
        return Err(ApiError::Unauthorized);
    };

    let age = chrono::Utc::now() - session.created_at;
    if age.num_days() > REFRESH_TOKEN_MAX_AGE_DAYS {
        revoke_session(&state.db, session.id).await?;
        return Err(ApiError::Unauthorized);
    }

    let user: Option<UserAuthRow> = sqlx::query_as(
        r#"
        SELECT id, password_hash, role::text as role, domain_id, is_active, totp_secret_enc
        FROM users WHERE id = $1
        "#,
    )
    .bind(session.user_id)
    .fetch_optional(&state.db)
    .await?;

    let Some(user) = user else {
        revoke_session(&state.db, session.id).await?;
        return Err(ApiError::Unauthorized);
    };
    if !user.is_active {
        revoke_session(&state.db, session.id).await?;
        return Err(ApiError::Unauthorized);
    }

    // Rotation: alte Session widerrufen, neue ausstellen (siehe
    // docs/architecture.md, Sicherheitsmodell: "Refresh-Tokens rotierend").
    revoke_session(&state.db, session.id).await?;

    issue_token_pair(&state, user.id, parse_role(&user.role), user.domain_id).await
}

pub async fn logout(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Json(req): Json<RefreshRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let hash = token::hash_token(&req.refresh_token);
    let session: Option<SessionRow> = sqlx::query_as(
        "SELECT id, user_id, created_at FROM sessions WHERE refresh_token_hash = $1",
    )
    .bind(&hash)
    .fetch_optional(&state.db)
    .await?;

    if let Some(session) = session {
        if !actor.owns(session.user_id) {
            // Fremde Session -> wie "nicht gefunden" behandeln, kein Hinweis
            // auf Existenz der Session eines anderen Nutzers.
            return Err(ApiError::NotFound);
        }
        revoke_session(&state.db, session.id).await?;
    }

    Ok(Json(serde_json::json!({ "status": "logged_out" })))
}

async fn revoke_session(pool: &sqlx::PgPool, session_id: Uuid) -> ApiResult<()> {
    sqlx::query("UPDATE sessions SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL")
        .bind(session_id)
        .execute(pool)
        .await?;
    Ok(())
}

async fn issue_token_pair(
    state: &AppState,
    user_id: Uuid,
    role: Role,
    domain_id: Uuid,
) -> ApiResult<Json<TokenResponse>> {
    let now = chrono::Utc::now().timestamp();
    let domain_scope = if role == Role::SuperAdmin {
        None
    } else {
        Some(domain_id)
    };

    let access_token = state
        .jwt
        .issue(user_id, role, domain_scope, now)
        .map_err(|e| ApiError::TokenIssue(e.to_string()))?;

    let (refresh_plaintext, refresh_hash) = token::generate_opaque_token();
    sqlx::query("INSERT INTO sessions (user_id, refresh_token_hash) VALUES ($1, $2)")
        .bind(user_id)
        .bind(&refresh_hash)
        .execute(&state.db)
        .await?;

    Ok(Json(TokenResponse {
        access_token,
        refresh_token: refresh_plaintext,
        token_type: "Bearer",
        expires_in: ACCESS_TOKEN_TTL_SECONDS,
    }))
}

fn parse_role(s: &str) -> Role {
    match s {
        "super_admin" => Role::SuperAdmin,
        "domain_admin" => Role::DomainAdmin,
        _ => Role::User,
    }
}
