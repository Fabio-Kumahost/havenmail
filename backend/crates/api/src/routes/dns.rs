//! DKIM-Schlüsselerzeugung, DNS-Prüfung und -Empfehlungen pro Domain.
//! Siehe docs/dns-setup.md für das Zielbild der Einträge.

use crate::auth_extractor::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    Json,
};
use havenmail_core::rbac::Action;
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, FromRow)]
struct DomainRow {
    name: String,
    dkim_selector: String,
}

async fn fetch_domain_or_404(pool: &sqlx::PgPool, domain_id: Uuid) -> ApiResult<DomainRow> {
    sqlx::query_as("SELECT name, dkim_selector FROM domains WHERE id = $1")
        .bind(domain_id)
        .fetch_optional(pool)
        .await?
        .ok_or(ApiError::NotFound)
}

#[derive(Debug, Serialize)]
pub struct DkimKeyResponse {
    pub selector: String,
    pub dns_record_name: String,
    pub dns_record_value: String,
}

/// Erzeugt (oder erneuert) den DKIM-Schlüssel einer Domain. Der private
/// Schlüssel wird nur verschlüsselt gespeichert, nie in der API-Antwort
/// zurückgegeben.
pub async fn generate_dkim_key(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(domain_id): Path<Uuid>,
) -> ApiResult<Json<DkimKeyResponse>> {
    if !actor.can(Action::ManageDomain, Some(domain_id)) {
        return Err(ApiError::Forbidden);
    }
    let domain = fetch_domain_or_404(&state.db, domain_id).await?;

    let generated = havenmail_core::dkim::generate_dkim_key()
        .map_err(|e| ApiError::TokenIssue(e.to_string()))?;
    let encrypted =
        havenmail_core::dkim::encrypt_private_key(&state.secrets_key, &generated.private_key_pem)
            .map_err(|e| ApiError::TokenIssue(e.to_string()))?;

    sqlx::query(
        r#"
        INSERT INTO dkim_keys (domain_id, selector, private_key_enc, public_key, active)
        VALUES ($1, $2, $3, $4, true)
        ON CONFLICT (domain_id, selector)
        DO UPDATE SET private_key_enc = EXCLUDED.private_key_enc, public_key = EXCLUDED.public_key, active = true
        "#,
    )
    .bind(domain_id)
    .bind(&domain.dkim_selector)
    .bind(&encrypted)
    .bind(&generated.dns_txt_value)
    .execute(&state.db)
    .await?;

    Ok(Json(DkimKeyResponse {
        selector: domain.dkim_selector.clone(),
        dns_record_name: format!("{}._domainkey.{}", domain.dkim_selector, domain.name),
        dns_record_value: generated.dns_txt_value,
    }))
}

#[derive(Debug, Serialize)]
pub struct DnsRecommendationsResponse {
    pub mx: DnsEntry,
    pub spf: DnsEntry,
    pub dkim: Option<DnsEntry>,
    pub dmarc: DnsEntry,
}

#[derive(Debug, Serialize)]
pub struct DnsEntry {
    pub record_type: &'static str,
    pub name: String,
    pub value: String,
}

pub async fn dns_recommendations(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(domain_id): Path<Uuid>,
) -> ApiResult<Json<DnsRecommendationsResponse>> {
    if !actor.can(Action::ManageDomain, Some(domain_id)) {
        return Err(ApiError::NotFound);
    }
    let domain = fetch_domain_or_404(&state.db, domain_id).await?;

    let dkim_public: Option<String> = sqlx::query_scalar(
        "SELECT public_key FROM dkim_keys WHERE domain_id = $1 AND active = true LIMIT 1",
    )
    .bind(domain_id)
    .fetch_optional(&state.db)
    .await?;

    let dmarc_report_address = format!("dmarc@{}", domain.name);
    let rec = havenmail_core::dns_check::recommend_dns_records(
        &state.mail_hostname,
        &domain.dkim_selector,
        dkim_public
            .as_deref()
            .unwrap_or("<noch nicht erzeugt — zuerst DKIM-Schlüssel generieren>"),
        &dmarc_report_address,
    );

    Ok(Json(DnsRecommendationsResponse {
        mx: DnsEntry {
            record_type: "MX",
            name: domain.name.clone(),
            value: rec.mx,
        },
        spf: DnsEntry {
            record_type: "TXT",
            name: domain.name.clone(),
            value: rec.spf,
        },
        dkim: dkim_public.map(|_| DnsEntry {
            record_type: "TXT",
            name: format!("{}.{}", rec.dkim_selector, domain.name),
            value: rec.dkim_value,
        }),
        dmarc: DnsEntry {
            record_type: "TXT",
            name: format!("_dmarc.{}", domain.name),
            value: rec.dmarc,
        },
    }))
}

#[derive(Debug, Serialize)]
pub struct DnsCheckResponse {
    pub results: Vec<havenmail_core::dns_check::DnsCheckResult>,
}

/// Führt die tatsächlichen DNS-Abfragen aus und vergleicht sie mit den
/// erwarteten Werten (MX, SPF, DKIM sofern erzeugt, DMARC).
pub async fn run_dns_check(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(domain_id): Path<Uuid>,
) -> ApiResult<Json<DnsCheckResponse>> {
    if !actor.can(Action::ManageDomain, Some(domain_id)) {
        return Err(ApiError::NotFound);
    }
    let domain = fetch_domain_or_404(&state.db, domain_id).await?;

    let mut results = Vec::new();
    results.push(havenmail_core::dns_check::check_mx(&domain.name, &state.mail_hostname).await);
    results
        .push(havenmail_core::dns_check::check_txt_contains("SPF", &domain.name, "v=spf1").await);
    results.push(
        havenmail_core::dns_check::check_txt_contains(
            "DMARC",
            &format!("_dmarc.{}", domain.name),
            "v=DMARC1",
        )
        .await,
    );

    let dkim_active: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM dkim_keys WHERE domain_id = $1 AND active = true)",
    )
    .bind(domain_id)
    .fetch_one(&state.db)
    .await?;
    if dkim_active {
        results.push(
            havenmail_core::dns_check::check_txt_contains(
                "DKIM",
                &format!("{}._domainkey.{}", domain.dkim_selector, domain.name),
                "v=DKIM1",
            )
            .await,
        );
    }

    // Ergebnisse zur Historie persistieren (docs/architecture.md, Datenmodell: dns_checks).
    for r in &results {
        let status_str = match r.status {
            havenmail_core::dns_check::CheckStatus::Ok => "ok",
            havenmail_core::dns_check::CheckStatus::Missing => "missing",
            havenmail_core::dns_check::CheckStatus::Mismatch => "mismatch",
        };
        sqlx::query(
            "INSERT INTO dns_checks (domain_id, record_type, expected, actual, status) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(domain_id)
        .bind(&r.record_type)
        .bind(&r.expected)
        .bind(&r.actual)
        .bind(status_str)
        .execute(&state.db)
        .await?;
    }

    Ok(Json(DnsCheckResponse { results }))
}
