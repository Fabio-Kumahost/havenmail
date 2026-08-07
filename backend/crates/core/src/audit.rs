//! Unveränderliches Audit-Log für administrative Änderungen.
//!
//! Jeder Eintrag verkettet seinen Hash mit dem Hash des Vorgängereintrags
//! (`prev_hash`/`hash`, analog einer einfachen Hash-Chain). Nachträgliches
//! Verändern oder Löschen eines Eintrags in der Mitte der Kette macht sich
//! beim Neuberechnen/Verifizieren der Kette bemerkbar. Ersetzt keine
//! kryptografische Signatur, erhöht aber die Hürde für unbemerkte
//! nachträgliche Manipulation deutlich gegenüber einer reinen Log-Tabelle.

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct AuditEntryInput {
    pub actor_id: Option<Uuid>,
    pub action: String,
    pub target: String,
    pub before: Option<Value>,
    pub after: Option<Value>,
    pub created_at_unix: i64,
}

/// Berechnet den Hash eines neuen Audit-Eintrags unter Einbeziehung des
/// Hashs des vorherigen Eintrags (`None` für den allerersten Eintrag der
/// Kette). Die Control-Plane speichert `hash` und `prev_hash` gemeinsam mit
/// dem Eintrag in `audit_log`.
pub fn compute_entry_hash(entry: &AuditEntryInput, prev_hash: Option<&str>) -> String {
    // Kanonische, stabil sortierte JSON-Repräsentation für deterministisches Hashing.
    let payload = serde_json::json!({
        "actor_id": entry.actor_id,
        "action": entry.action,
        "target": entry.target,
        "before": entry.before,
        "after": entry.after,
        "created_at_unix": entry.created_at_unix,
        "prev_hash": prev_hash,
    });
    let mut hasher = Sha256::new();
    hasher.update(payload.to_string().as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Ein gespeicherter Audit-Eintrag, wie er aus `audit_log` gelesen wird —
/// für die Verifikation der Kette über eine Sequenz von Einträgen.
#[derive(Debug, Clone)]
pub struct StoredAuditEntry {
    pub input: AuditEntryInput,
    pub prev_hash: Option<String>,
    pub hash: String,
}

/// Prüft eine chronologisch sortierte Folge von Audit-Einträgen auf
/// Unversehrtheit der Kette. `Ok(())` heißt: kein Eintrag wurde nachträglich
/// verändert oder aus der Mitte entfernt.
pub fn verify_chain(entries: &[StoredAuditEntry]) -> Result<(), AuditChainError> {
    let mut expected_prev: Option<&str> = None;
    for (index, entry) in entries.iter().enumerate() {
        if entry.prev_hash.as_deref() != expected_prev {
            return Err(AuditChainError::BrokenLink { index });
        }
        let recomputed = compute_entry_hash(&entry.input, entry.prev_hash.as_deref());
        if recomputed != entry.hash {
            return Err(AuditChainError::TamperedEntry { index });
        }
        expected_prev = Some(entry.hash.as_str());
    }
    Ok(())
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AuditChainError {
    #[error("Kette unterbrochen bei Eintrag {index}: prev_hash passt nicht zum Vorgänger")]
    BrokenLink { index: usize },
    #[error("Eintrag {index} wurde nachträglich verändert (Hash stimmt nicht)")]
    TamperedEntry { index: usize },
}

/// Feste Advisory-Lock-ID (beliebig gewählt, aber stabil) zur Serialisierung
/// von Audit-Anhängen über `record`. Ohne diese Sperre könnten zwei
/// gleichzeitige Requests denselben `prev_hash` lesen und die Kette
/// verzweigen lassen, statt sie linear zu verlängern.
const AUDIT_CHAIN_LOCK_KEY: i64 = 0x4841_5645_4e4d_4c31; // "HAVENML1" in Hex

/// Hängt einen neuen Eintrag an die Audit-Log-Hash-Chain in der Datenbank
/// an. Serialisiert über einen Postgres-Advisory-Lock (in derselben
/// Transaktion gehalten), damit gleichzeitige Aufrufe die Kette nicht
/// verzweigen lassen. `ip` als Text im Postgres-`inet`-Format (z. B. aus
/// `client_ip`-Modul der API).
#[allow(clippy::too_many_arguments)]
pub async fn record(
    pool: &sqlx::PgPool,
    actor_id: Option<Uuid>,
    action: &str,
    target: &str,
    domain_id: Option<Uuid>,
    before: Option<Value>,
    after: Option<Value>,
    ip: Option<&str>,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(AUDIT_CHAIN_LOCK_KEY)
        .execute(&mut *tx)
        .await?;

    let prev_hash: Option<String> =
        sqlx::query_scalar("SELECT hash FROM audit_log ORDER BY seq DESC LIMIT 1")
            .fetch_optional(&mut *tx)
            .await?;

    let created_at_unix = chrono::Utc::now().timestamp();
    let input = AuditEntryInput {
        actor_id,
        action: action.to_string(),
        target: target.to_string(),
        before,
        after,
        created_at_unix,
    };
    let hash = compute_entry_hash(&input, prev_hash.as_deref());

    sqlx::query(
        r#"
        INSERT INTO audit_log (actor_id, actor_id_snapshot, action, target, domain_id, before, after, ip, prev_hash, hash)
        VALUES ($1, $1, $2, $3, $4, $5, $6, $7::inet, $8, $9)
        "#,
    )
    .bind(input.actor_id)
    .bind(&input.action)
    .bind(&input.target)
    .bind(domain_id)
    .bind(&input.before)
    .bind(&input.after)
    .bind(ip)
    .bind(&prev_hash)
    .bind(&hash)
    .execute(&mut *tx)
    .await?;

    tx.commit().await
}

/// Lädt die vollständige Audit-Log-Kette in Einfüge-Reihenfolge (`seq`) für
/// `verify_chain`. Nutzt bewusst `actor_id_snapshot` statt der live-FK
/// `actor_id` — letztere kann durch `ON DELETE SET NULL` nach dem
/// Einfügen mutieren (siehe Migration 0016), `actor_id_snapshot` nicht.
pub async fn load_chain_entries(pool: &sqlx::PgPool) -> Result<Vec<StoredAuditEntry>, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct Row {
        actor_id_snapshot: Option<Uuid>,
        action: String,
        target: String,
        before: Option<Value>,
        after: Option<Value>,
        created_at: chrono::DateTime<chrono::Utc>,
        prev_hash: Option<String>,
        hash: String,
    }

    let rows = sqlx::query_as::<_, Row>(
        r#"
        SELECT actor_id_snapshot, action, target, before, after, created_at, prev_hash, hash
        FROM audit_log
        ORDER BY seq ASC
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| StoredAuditEntry {
            input: AuditEntryInput {
                actor_id: r.actor_id_snapshot,
                action: r.action,
                target: r.target,
                before: r.before,
                after: r.after,
                created_at_unix: r.created_at.timestamp(),
            },
            prev_hash: r.prev_hash,
            hash: r.hash,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry(action: &str, created_at_unix: i64) -> AuditEntryInput {
        AuditEntryInput {
            actor_id: Some(Uuid::from_bytes([1; 16])),
            action: action.to_string(),
            target: "domain:example.org".to_string(),
            before: None,
            after: Some(serde_json::json!({ "is_active": true })),
            created_at_unix,
        }
    }

    fn build_chain(actions: &[&str]) -> Vec<StoredAuditEntry> {
        let mut entries = Vec::new();
        let mut prev_hash: Option<String> = None;
        for (i, action) in actions.iter().enumerate() {
            let input = sample_entry(action, 1_700_000_000 + i as i64);
            let hash = compute_entry_hash(&input, prev_hash.as_deref());
            entries.push(StoredAuditEntry {
                input,
                prev_hash: prev_hash.clone(),
                hash: hash.clone(),
            });
            prev_hash = Some(hash);
        }
        entries
    }

    #[test]
    fn valid_chain_verifies_ok() {
        let chain = build_chain(&["domain.create", "domain.update", "user.create"]);
        assert!(verify_chain(&chain).is_ok());
    }

    #[test]
    fn detects_tampered_entry() {
        let mut chain = build_chain(&["domain.create", "domain.update"]);
        chain[0].input.action = "domain.delete".to_string(); // nachträglich manipuliert
        assert_eq!(
            verify_chain(&chain),
            Err(AuditChainError::TamperedEntry { index: 0 })
        );
    }

    #[test]
    fn detects_removed_middle_entry() {
        let chain = build_chain(&["a", "b", "c"]);
        let with_gap: Vec<_> = chain
            .into_iter()
            .enumerate()
            .filter(|(i, _)| *i != 1)
            .map(|(_, e)| e)
            .collect();
        assert!(matches!(
            verify_chain(&with_gap),
            Err(AuditChainError::BrokenLink { index: 1 })
        ));
    }

    #[test]
    fn same_content_different_position_yields_different_hash() {
        // Stellt sicher, dass prev_hash tatsächlich in den Hash einfließt
        // (sonst könnten Einträge umsortiert werden, ohne dass es auffällt).
        let input = sample_entry("domain.create", 1_700_000_000);
        let hash_as_first = compute_entry_hash(&input, None);
        let hash_as_second = compute_entry_hash(&input, Some("deadbeef"));
        assert_ne!(hash_as_first, hash_as_second);
    }
}
