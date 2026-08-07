use crate::auth_extractor::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::routes::security_settings;
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
    let min_len = security_settings::min_password_length(&state.db).await?;
    let local_part = req.local_part.trim().to_lowercase();
    // Zeichen-Whitelist statt nur "nicht leer" — local_part landet
    // ungeprüft in Dovecots %n-Pfad-Expansion (mail_location); ohne diese
    // Prüfung wäre Path-Traversal außerhalb des Maildir-Bereichs möglich
    // (siehe havenmail_core::validation).
    if !havenmail_core::validation::is_valid_mailbox_local_part(&local_part)
        || (req.password.len() as i32) < min_len
    {
        return Err(ApiError::BadRequest(format!(
            "local_part ungültig, Passwort muss mindestens {min_len} Zeichen haben"
        )));
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
    .bind(&local_part)
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

#[derive(Debug, Serialize)]
pub struct UserStorage {
    pub id: Uuid,
    /// `None`, wenn die Mailbox noch nie ein IMAP-Login hatte (Maildir
    /// existiert dann noch nicht, siehe havenmail_core::mailbox_storage).
    pub bytes: Option<i64>,
}

/// Separater Endpunkt statt Erweiterung von `list_users`: `du -sb` pro
/// Postfach kostet spürbar mehr Zeit als die reine DB-Abfrage — Domains
/// mit vielen Postfächern sollen die schnelle Liste nicht auf die
/// langsamste Mailbox warten lassen. Das Frontend lädt Storage separat
/// nach und merged per `id`.
pub async fn get_users_storage(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(domain_id): Path<Uuid>,
) -> ApiResult<Json<Vec<UserStorage>>> {
    if !actor.can(Action::ManageDomainUsers, Some(domain_id)) {
        return Err(ApiError::Forbidden);
    }
    let domain_name: String = sqlx::query_scalar("SELECT name FROM domains WHERE id = $1")
        .bind(domain_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;

    let users: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, local_part FROM users WHERE domain_id = $1 ORDER BY local_part")
            .bind(domain_id)
            .fetch_all(&state.db)
            .await?;

    let mail_base =
        std::env::var("HAVENMAIL_MAIL_DIR").unwrap_or_else(|_| "/var/mail/havenmail".to_string());

    let mut out = Vec::with_capacity(users.len());
    for (id, local_part) in users {
        let path = std::path::Path::new(&mail_base)
            .join(&domain_name)
            .join(&local_part);
        let bytes = havenmail_core::mailbox_storage::usage_bytes(&path).await;
        out.push(UserStorage { id, bytes });
    }
    Ok(Json(out))
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
        let min_len = security_settings::min_password_length(&state.db).await?;
        if (pw.len() as i32) < min_len {
            return Err(ApiError::BadRequest(format!(
                "Passwort muss mindestens {min_len} Zeichen haben"
            )));
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
    let min_len = security_settings::min_password_length(&state.db).await?;
    if (req.new_password.len() as i32) < min_len {
        return Err(ApiError::BadRequest(format!(
            "neues Passwort muss mindestens {min_len} Zeichen haben"
        )));
    }

    let row: PasswordHashRow = sqlx::query_as("SELECT password_hash FROM users WHERE id = $1")
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

/// Eine Zeile aus der Import-CSV. `role`/`quota_bytes` optional (leer =
/// Default "user"/kein Limit) — dieselbe Semantik wie `CreateUserRequest`.
#[derive(Debug, Deserialize)]
struct ImportRow {
    local_part: String,
    password: String,
    #[serde(default)]
    role: String,
    #[serde(default)]
    quota_bytes: String,
}

#[derive(Debug, Serialize)]
pub struct ImportRowError {
    /// 1-basiert, zählt die Kopfzeile nicht mit (Zeile 1 der Daten = 1,
    /// nicht 2) — passt zur Zeilennummer, die ein Nutzer beim Öffnen der
    /// CSV in einem Editor/Tabellenkalkulation sieht, wenn man die
    /// Kopfzeile mitzählt und 1-basiert bleibt.
    pub row: usize,
    pub local_part: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ImportResponse {
    pub created: Vec<User>,
    pub errors: Vec<ImportRowError>,
}

#[derive(Debug, Deserialize)]
pub struct ImportRequest {
    /// Rohtext der CSV-Datei, Kopfzeile erwartet:
    /// local_part,password,role,quota_bytes (role/quota_bytes optional,
    /// leer lassen für Default).
    pub csv: String,
}

/// Importiert Postfächer aus CSV — Zeile für Zeile, nicht alles-oder-
/// nichts: eine fehlerhafte Zeile (z. B. Adresse existiert schon, zu
/// kurzes Passwort) überspringt nur diese und macht mit der nächsten
/// weiter, damit ein einzelner Tippfehler nicht den ganzen Import einer
/// großen Domain verhindert. Nutzt dieselbe Validierung wie `create_user`
/// (Passwortlänge, Rollen-Rechteausweitungsschutz), aber inline statt als
/// gemeinsame Funktion — der Unterschied "ein ApiError abbricht" vs. "ein
/// Fehler wird gesammelt und weitergemacht" hätte die Extraktion selbst
/// komplizierter gemacht als die kleine Duplizierung.
pub async fn import_users(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(domain_id): Path<Uuid>,
    headers: HeaderMap,
    Json(req): Json<ImportRequest>,
) -> ApiResult<Json<ImportResponse>> {
    if !actor.can(Action::ManageDomainUsers, Some(domain_id)) {
        return Err(ApiError::Forbidden);
    }
    // Einmal vor der Schleife lesen statt pro Zeile — die Richtlinie ändert
    // sich nicht während ein einzelner Import läuft, und ein CSV kann
    // hunderte Zeilen haben.
    let min_len = security_settings::min_password_length(&state.db).await?;

    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(req.csv.as_bytes());

    let mut created = Vec::new();
    let mut errors = Vec::new();

    for (idx, result) in reader.deserialize::<ImportRow>().enumerate() {
        let row_num = idx + 1;
        let row = match result {
            Ok(r) => r,
            Err(e) => {
                errors.push(ImportRowError {
                    row: row_num,
                    local_part: String::new(),
                    message: format!("Zeile konnte nicht gelesen werden: {e}"),
                });
                continue;
            }
        };

        let local_part = row.local_part.trim().to_lowercase();
        // Zeichen-Whitelist wie in create_user (siehe dortiger Kommentar) —
        // auch der CSV-Bulk-Import muss dieselbe Path-Traversal-Prüfung
        // durchlaufen, nicht nur der Einzel-Erstellungs-Endpunkt.
        if !havenmail_core::validation::is_valid_mailbox_local_part(&local_part)
            || (row.password.len() as i32) < min_len
        {
            errors.push(ImportRowError {
                row: row_num,
                local_part,
                message: format!(
                    "local_part ungültig, Passwort muss mindestens {min_len} Zeichen haben"
                ),
            });
            continue;
        }

        let requested_role = if row.role.trim().is_empty() {
            "user"
        } else {
            row.role.trim()
        };
        if requested_role == "super_admin" && actor.role != Role::SuperAdmin {
            errors.push(ImportRowError {
                row: row_num,
                local_part,
                message: "keine Berechtigung, super_admin-Konten anzulegen".to_string(),
            });
            continue;
        }
        if !["super_admin", "domain_admin", "user"].contains(&requested_role) {
            errors.push(ImportRowError {
                row: row_num,
                local_part,
                message: "ungültige Rolle".to_string(),
            });
            continue;
        }

        let quota_bytes: Option<i64> = if row.quota_bytes.trim().is_empty() {
            None
        } else {
            match row.quota_bytes.trim().parse() {
                Ok(v) => Some(v),
                Err(_) => {
                    errors.push(ImportRowError {
                        row: row_num,
                        local_part,
                        message: "quota_bytes muss eine Zahl sein".to_string(),
                    });
                    continue;
                }
            }
        };

        let password_hash = match password::hash_password(&row.password) {
            Ok(h) => h,
            Err(e) => {
                errors.push(ImportRowError {
                    row: row_num,
                    local_part,
                    message: format!("Passwort-Hashing fehlgeschlagen: {e}"),
                });
                continue;
            }
        };

        let insert_result: Result<User, sqlx::Error> = sqlx::query_as(
            r#"
            INSERT INTO users (domain_id, local_part, password_hash, role, quota_bytes)
            VALUES ($1, $2, $3, $4::havenmail_user_role, $5)
            RETURNING id, domain_id, local_part, role::text as role, quota_bytes, is_active, created_at
            "#,
        )
        .bind(domain_id)
        .bind(&local_part)
        .bind(&password_hash)
        .bind(requested_role)
        .bind(quota_bytes)
        .fetch_one(&state.db)
        .await;

        match insert_result {
            Ok(user) => created.push(user),
            Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => {
                errors.push(ImportRowError {
                    row: row_num,
                    local_part,
                    message: "Postfach existiert bereits".to_string(),
                });
            }
            Err(e) => {
                errors.push(ImportRowError {
                    row: row_num,
                    local_part,
                    message: format!("Datenbankfehler: {e}"),
                });
            }
        }
    }

    crate::audit_log::log(
        &state,
        &actor,
        &headers,
        "user.bulk_import",
        &domain_id.to_string(),
        Some(domain_id),
        None,
        Some(serde_json::json!({ "created": created.len(), "errors": errors.len() })),
    )
    .await;

    Ok(Json(ImportResponse { created, errors }))
}

/// Exportiert alle Postfächer einer Domain als CSV — niemals
/// password_hash oder sonst etwas Geheimes, nur was auch `list_users`
/// zeigt. `role`/`quota_bytes` im selben Format wie der Import erwartet,
/// damit Export→Bearbeiten→Import derselben Domain (oder einer neuen)
/// funktioniert (Passwort-Spalte bleibt beim Export leer, da der Hash
/// nicht rückgängig gemacht werden kann — ein reiner Export ist also kein
/// vollständiges Backup der Zugangsdaten, nur der Struktur).
pub async fn export_users(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(domain_id): Path<Uuid>,
) -> ApiResult<axum::response::Response> {
    use axum::http::header;
    use axum::response::IntoResponse;

    if !actor.can(Action::ManageDomainUsers, Some(domain_id)) {
        return Err(ApiError::Forbidden);
    }

    let users: Vec<User> = sqlx::query_as(
        "SELECT id, domain_id, local_part, role::text as role, quota_bytes, is_active, created_at FROM users WHERE domain_id = $1 ORDER BY local_part",
    )
    .bind(domain_id)
    .fetch_all(&state.db)
    .await?;

    // Reines In-Memory-Schreiben (Vec<u8>, kein I/O) — schlägt praktisch
    // nie fehl, ein erzwungener Umweg über einen der bestehenden
    // ApiError-Varianten (alle für DB-/Token-Fehler gedacht) wäre hier
    // irreführender als ein `expect`.
    let mut writer = csv::WriterBuilder::new().from_writer(vec![]);
    writer
        .write_record(["local_part", "password", "role", "quota_bytes", "is_active"])
        .expect("CSV-Schreiben in einen Vec<u8> kann nicht fehlschlagen");
    for user in &users {
        writer
            .write_record([
                user.local_part.as_str(),
                "",
                user.role.as_str(),
                &user.quota_bytes.map(|q| q.to_string()).unwrap_or_default(),
                &user.is_active.to_string(),
            ])
            .expect("CSV-Schreiben in einen Vec<u8> kann nicht fehlschlagen");
    }
    let body = writer
        .into_inner()
        .expect("CSV-Schreiben in einen Vec<u8> kann nicht fehlschlagen");

    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"postfaecher.csv\"",
            ),
        ],
        body,
    )
        .into_response())
}
