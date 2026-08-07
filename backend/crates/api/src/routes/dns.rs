//! DKIM-Schlüsselerzeugung, DNS-Prüfung und -Empfehlungen pro Domain.
//! Siehe docs/dns-setup.md für das Zielbild der Einträge.

use crate::auth_extractor::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::HeaderMap,
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
    pub active: bool,
}

#[derive(Debug, Serialize, FromRow)]
pub struct DkimKeyListEntry {
    pub selector: String,
    pub active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Verzeichnis der entschlüsselt auf Platte liegenden privaten Schlüssel
/// (nur die AKTIVEN werden dort tatsächlich gebraucht, siehe
/// `apply_dkim_maps` unten — ein gerade erzeugter, noch nicht aktivierter
/// Schlüssel liegt trotzdem schon dort, damit die Aktivierung selbst kein
/// Krypto-Handling mehr braucht, nur noch die beiden Map-Dateien).
fn dkim_dir() -> std::path::PathBuf {
    std::env::var("HAVENMAIL_DKIM_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/etc/havenmail/dkim"))
}

/// Erzeugt einen NEUEN DKIM-Schlüssel mit eigenem, zeitstempelbasiertem
/// Selektor (Rotation ohne den bisherigen aktiven Schlüssel sofort zu
/// ersetzen — Empfänger, die den alten öffentlichen Schlüssel noch
/// gecacht haben, dürfen mit der bisherigen Signatur weiter validieren
/// können, bis der neue DNS-TXT-Eintrag propagiert ist). Ausnahme: hat die
/// Domain noch GAR keinen Schlüssel, wird der erste sofort aktiv — es gibt
/// nichts zu schützen, wenn vorher nichts signiert wurde.
pub async fn generate_dkim_key(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(domain_id): Path<Uuid>,
    headers: HeaderMap,
) -> ApiResult<Json<DkimKeyResponse>> {
    if !actor.can(Action::ManageDomain, Some(domain_id)) {
        return Err(ApiError::Forbidden);
    }
    let domain = fetch_domain_or_404(&state.db, domain_id).await?;

    let has_any_key: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM dkim_keys WHERE domain_id = $1)")
            .bind(domain_id)
            .fetch_one(&state.db)
            .await?;
    let is_first_key = !has_any_key;

    // Zeitstempelbasiert statt fortlaufender Nummer — kollisionsfrei ohne
    // eine weitere Abfrage, und der Zeitpunkt der Erzeugung ist am
    // Selektor selbst schon ablesbar.
    let selector = format!("dkim{}", chrono::Utc::now().format("%Y%m%d%H%M%S"));

    let generated = havenmail_core::dkim::generate_dkim_key()
        .map_err(|e| ApiError::TokenIssue(e.to_string()))?;
    let encrypted =
        havenmail_core::dkim::encrypt_private_key(&state.secrets_key, &generated.private_key_pem)
            .map_err(|e| ApiError::TokenIssue(e.to_string()))?;

    sqlx::query(
        "INSERT INTO dkim_keys (domain_id, selector, private_key_enc, public_key, active) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(domain_id)
    .bind(&selector)
    .bind(&encrypted)
    .bind(&generated.dns_txt_value)
    .bind(is_first_key)
    .execute(&state.db)
    .await?;

    // Privatschlüssel schon jetzt entschlüsselt auf Platte ablegen (0640,
    // nur havenmail/dovecot lesbar — Verzeichnis wird analog zu
    // ReadWritePaths unten mit denselben Rechten wie /var/mail gehalten).
    // Aktivierung selbst muss dann nur noch die Maps neu schreiben, kein
    // erneutes Entschlüsseln.
    let key_path = havenmail_core::dkim_apply::key_file_path(&dkim_dir(), &domain.name, &selector);
    if let Some(parent) = key_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| ApiError::BadRequest(format!("DKIM-Verzeichnis fehlt: {e}")))?;
    }
    std::fs::write(&key_path, &generated.private_key_pem).map_err(|e| {
        ApiError::BadRequest(format!(
            "Privatschlüssel konnte nicht geschrieben werden: {e}"
        ))
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o640));
    }

    if is_first_key {
        sqlx::query("UPDATE domains SET dkim_selector = $1 WHERE id = $2")
            .bind(&selector)
            .bind(domain_id)
            .execute(&state.db)
            .await?;
        apply_dkim_maps(&state).await?;
    }

    let response = DkimKeyResponse {
        selector: selector.clone(),
        dns_record_name: format!("{selector}._domainkey.{}", domain.name),
        dns_record_value: generated.dns_txt_value,
        active: is_first_key,
    };

    // Nur den öffentlichen DNS-TXT-Wert protokollieren (ohnehin zur
    // Veröffentlichung bestimmt) — der private Schlüssel taucht hier nie auf.
    crate::audit_log::log(
        &state,
        &actor,
        &headers,
        "dkim.generate",
        &domain_id.to_string(),
        Some(domain_id),
        None,
        serde_json::to_value(&response).ok(),
    )
    .await;

    Ok(Json(response))
}

/// Historie/Alter aller je erzeugten Schlüssel einer Domain — für die
/// altersbasierte Anzeige im Panel und um pending (noch nicht aktive)
/// Rotationsschlüssel zum Aktivieren aufzulisten.
pub async fn list_dkim_keys(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(domain_id): Path<Uuid>,
) -> ApiResult<Json<Vec<DkimKeyListEntry>>> {
    if !actor.can(Action::ManageDomain, Some(domain_id)) {
        return Err(ApiError::NotFound);
    }
    let keys: Vec<DkimKeyListEntry> = sqlx::query_as(
        "SELECT selector, active, created_at FROM dkim_keys WHERE domain_id = $1 ORDER BY created_at DESC",
    )
    .bind(domain_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(keys))
}

/// Macht einen zuvor per `generate_dkim_key` erzeugten (pending) Schlüssel
/// zum aktiven Signierschlüssel der Domain — erst hier wird die
/// tatsächlich wirksame Rspamd-Konfiguration (selector_map/path_map) neu
/// geschrieben. Der Admin ruft das idealerweise erst auf, nachdem der
/// neue DNS-TXT-Eintrag propagiert ist (siehe DkimKeyResponse beim
/// Erzeugen).
pub async fn activate_dkim_key(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path((domain_id, selector)): Path<(Uuid, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<serde_json::Value>> {
    if !actor.can(Action::ManageDomain, Some(domain_id)) {
        return Err(ApiError::NotFound);
    }
    let domain = fetch_domain_or_404(&state.db, domain_id).await?;

    let mut tx = state.db.begin().await?;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM dkim_keys WHERE domain_id = $1 AND selector = $2)",
    )
    .bind(domain_id)
    .bind(&selector)
    .fetch_one(&mut *tx)
    .await?;
    if !exists {
        return Err(ApiError::NotFound);
    }
    sqlx::query("UPDATE dkim_keys SET active = false WHERE domain_id = $1")
        .bind(domain_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE dkim_keys SET active = true WHERE domain_id = $1 AND selector = $2")
        .bind(domain_id)
        .bind(&selector)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE domains SET dkim_selector = $1 WHERE id = $2")
        .bind(&selector)
        .bind(domain_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    apply_dkim_maps(&state).await?;

    crate::audit_log::log(
        &state,
        &actor,
        &headers,
        "dkim.activate",
        &domain_id.to_string(),
        Some(domain_id),
        None,
        Some(serde_json::json!({ "domain": domain.name, "selector": selector })),
    )
    .await;

    Ok(Json(
        serde_json::json!({ "status": "aktiviert", "selector": selector }),
    ))
}

/// Rendert `selector_map`/`path_map` aus ALLEN aktuell aktiven
/// DKIM-Schlüsseln domänenübergreifend neu, prüft mit `rspamadm
/// configtest` und stößt bei Erfolg einen Reload an — dasselbe
/// Sicherheitsnetz-Muster wie `security_settings::apply_to_rspamd`. Wird
/// bei jeder Aktivierung sowie beim allerersten Schlüssel einer Domain
/// aufgerufen, liest aber immer den kompletten, aktuellen Bestand (nicht
/// nur die eine gerade geänderte Domain), da beide Dateien
/// domänenübergreifend sind.
async fn apply_dkim_maps(state: &AppState) -> ApiResult<()> {
    // Dieselbe Sperre wie `security_settings::apply_to_rspamd` — beide
    // Funktionen lesen/schreiben teils dieselben Rspamd-Dateien und dürfen
    // sich nicht verschränken (TOCTOU, siehe AppState::mail_config_lock).
    let _config_guard = state.mail_config_lock.lock().await;
    let dir = dkim_dir();
    let active_keys: Vec<(String, String)> = sqlx::query_as(
        "SELECT d.name, k.selector FROM dkim_keys k JOIN domains d ON d.id = k.domain_id WHERE k.active = true",
    )
    .fetch_all(&state.db)
    .await?;
    let entries: Vec<havenmail_core::dkim_apply::ActiveDkimKey> = active_keys
        .into_iter()
        .map(
            |(domain_name, selector)| havenmail_core::dkim_apply::ActiveDkimKey {
                domain_name,
                selector,
            },
        )
        .collect();

    let selector_map = havenmail_core::dkim_apply::render_selector_map(&entries);
    let path_map = havenmail_core::dkim_apply::render_path_map(&dir, &entries);

    let selector_map_path = dir.join("selectors.map");
    let path_map_path = dir.join("keys.map");
    std::fs::create_dir_all(&dir)
        .map_err(|e| ApiError::BadRequest(format!("DKIM-Verzeichnis fehlt: {e}")))?;

    let backup_selector = std::fs::read_to_string(&selector_map_path).ok();
    let backup_path = std::fs::read_to_string(&path_map_path).ok();
    std::fs::write(&selector_map_path, &selector_map).map_err(|e| {
        ApiError::BadRequest(format!(
            "selectors.map konnte nicht geschrieben werden: {e}"
        ))
    })?;
    std::fs::write(&path_map_path, &path_map).map_err(|e| {
        ApiError::BadRequest(format!("keys.map konnte nicht geschrieben werden: {e}"))
    })?;

    let configtest = tokio::process::Command::new("rspamadm")
        .arg("configtest")
        .output()
        .await;
    // Anders als bei security_settings::apply_to_rspamd (dort läuft
    // rspamadm auf jedem Havenmail-Server garantiert, siehe dortiger
    // Kommentar) wird DIESER Pfad schon beim allerersten DKIM-Schlüssel
    // einer frisch angelegten Domain durchlaufen — auch in Umgebungen ohne
    // installiertes Rspamd (CI-Tests, siehe api_integration.rs). Ein
    // fehlendes `rspamadm`-Binary (NotFound) ist dort kein
    // Konfigurationsfehler, sondern schlicht "kein Rspamd vorhanden" —
    // die geschriebenen Map-Dateien bleiben dann unverifiziert stehen,
    // statt die Anfrage mit 400 abzulehnen. Meldet `rspamadm` sich
    // tatsächlich zu Wort und lehnt ab, bleibt das weiterhin ein harter
    // Fehler mit Rollback (echter Konfigurationsfehler).
    let binary_missing = matches!(
        &configtest,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound
    );
    let configtest_ok =
        binary_missing || matches!(&configtest, Ok(output) if output.status.success());
    if !configtest_ok {
        match backup_selector {
            Some(c) => {
                let _ = std::fs::write(&selector_map_path, c);
            }
            None => {
                let _ = std::fs::remove_file(&selector_map_path);
            }
        }
        match backup_path {
            Some(c) => {
                let _ = std::fs::write(&path_map_path, c);
            }
            None => {
                let _ = std::fs::remove_file(&path_map_path);
            }
        }
        let detail = match configtest {
            Ok(output) => String::from_utf8_lossy(&output.stderr).trim().to_string(),
            Err(e) => e.to_string(),
        };
        return Err(ApiError::BadRequest(format!(
            "Rspamd-Konfiguration nach DKIM-Änderung ungültig, verworfen: {detail}"
        )));
    }

    // Bewusst nicht-fatal (nur geloggt, kein Err-Return): anders als ein
    // Configtest-Fehschlag bedeutet ein fehlgeschlagener Trigger keinen
    // ungültigen Zustand — die Map-Dateien sind zu diesem Zeitpunkt schon
    // korrekt geschrieben und verifiziert, es fehlt bestenfalls der
    // sofortige Reload (etwa weil HAVENMAIL_STATE_DIR in einer
    // Umgebung ohne installierten Havenmail-Server, z. B. CI-Tests, gar
    // nicht existiert). Ein späterer Reload holt den Stand ohnehin nach.
    let state_dir =
        std::env::var("HAVENMAIL_STATE_DIR").unwrap_or_else(|_| "/var/lib/havenmail".to_string());
    let trigger_path = std::path::PathBuf::from(format!("{state_dir}/rspamd-reload-trigger"));
    if let Err(e) =
        havenmail_core::trigger_file::write(&trigger_path, &chrono::Utc::now().to_rfc3339())
    {
        eprintln!("Warnung: Rspamd-Reload-Trigger nach DKIM-Änderung fehlgeschlagen: {e}");
    }

    Ok(())
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
