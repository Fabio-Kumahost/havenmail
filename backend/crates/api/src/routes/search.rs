//! Domänenübergreifende Suche über Postfächer/Domains — man muss nicht
//! mehr vorher wissen, in welcher Domain ein Postfach liegt (relevant seit
//! es die Reseller-Übersicht gibt, siehe `routes/domains.rs::domains_overview`).
//! `super_admin` durchsucht alles, `domain_admin` ist wie überall sonst auf
//! die eigene Domain beschränkt.

use crate::auth_extractor::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::extract::{Query, State};
use axum::Json;
use havenmail_core::rbac::Role;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    q: String,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SearchResult {
    Domain {
        domain_id: Uuid,
        domain_name: String,
    },
    User {
        user_id: Uuid,
        domain_id: Uuid,
        domain_name: String,
        local_part: String,
    },
}

#[derive(Debug, FromRow)]
struct DomainMatch {
    id: Uuid,
    name: String,
}

#[derive(Debug, FromRow)]
struct UserMatch {
    id: Uuid,
    domain_id: Uuid,
    domain_name: String,
    local_part: String,
}

/// Maximal 20 Treffer je Kategorie — eine Live-Suche mit jedem Tastendruck
/// soll nicht auf einen unbegrenzten Full-Table-Scan mit potenziell
/// riesigem Ergebnis warten müssen.
const MAX_RESULTS_PER_KIND: i64 = 20;

pub async fn search(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Query(query): Query<SearchQuery>,
) -> ApiResult<Json<Vec<SearchResult>>> {
    let q = query.q.trim();
    if q.len() < 2 {
        // Zu kurze Anfragen liefern absichtlich nichts statt eines
        // riesigen "%%"-Treffers über die ganze Tabelle.
        return Ok(Json(vec![]));
    }
    let pattern = format!("%{}%", q.replace('%', "\\%").replace('_', "\\_"));

    let scoped_domain_id = match actor.role {
        Role::SuperAdmin => None,
        Role::DomainAdmin => match actor.domain_id {
            Some(id) => Some(id),
            None => return Ok(Json(vec![])),
        },
        Role::User => return Err(ApiError::Forbidden),
    };

    let mut results = Vec::new();

    // Domains nur für super_admin durchsuchen — für domain_admin ist die
    // eigene Domain ohnehin schon bekannt, kein Mehrwert, sie in den
    // Ergebnissen zu wiederholen.
    if scoped_domain_id.is_none() {
        let domains: Vec<DomainMatch> = sqlx::query_as(
            "SELECT id, name FROM domains WHERE name ILIKE $1 ORDER BY name LIMIT $2",
        )
        .bind(&pattern)
        .bind(MAX_RESULTS_PER_KIND)
        .fetch_all(&state.db)
        .await?;
        results.extend(domains.into_iter().map(|d| SearchResult::Domain {
            domain_id: d.id,
            domain_name: d.name,
        }));
    }

    let users: Vec<UserMatch> = match scoped_domain_id {
        Some(domain_id) => {
            sqlx::query_as(
                r#"
                SELECT u.id, u.domain_id, d.name as domain_name, u.local_part
                FROM users u JOIN domains d ON d.id = u.domain_id
                WHERE u.domain_id = $1 AND (u.local_part ILIKE $2 OR d.name ILIKE $2)
                ORDER BY u.local_part LIMIT $3
                "#,
            )
            .bind(domain_id)
            .bind(&pattern)
            .bind(MAX_RESULTS_PER_KIND)
            .fetch_all(&state.db)
            .await?
        }
        None => {
            sqlx::query_as(
                r#"
                SELECT u.id, u.domain_id, d.name as domain_name, u.local_part
                FROM users u JOIN domains d ON d.id = u.domain_id
                WHERE u.local_part ILIKE $1
                   OR d.name ILIKE $1
                   OR (u.local_part || '@' || d.name) ILIKE $1
                ORDER BY d.name, u.local_part LIMIT $2
                "#,
            )
            .bind(&pattern)
            .bind(MAX_RESULTS_PER_KIND)
            .fetch_all(&state.db)
            .await?
        }
    };
    results.extend(users.into_iter().map(|u| SearchResult::User {
        user_id: u.id,
        domain_id: u.domain_id,
        domain_name: u.domain_name,
        local_part: u.local_part,
    }));

    Ok(Json(results))
}
