//! Postfix-Warteschlange: Größe (für Snapshots), Auflistung und Löschung
//! einzelner/aller Einträge (Admin-Panel "Mail-Warteschlange").
//!
//! `postqueue` ist setgid `postdrop` und darf von jedem lokalen Benutzer
//! gelesen werden — Auflistung braucht keine besonderen Rechte. `postsuper
//! -d` ("use of this command is reserved for the superuser", live gegen
//! diese Installation geprüft) braucht dagegen root, das der `havenmail`-
//! Systembenutzer bewusst nicht hat. Löschung läuft daher wie der Rspamd-
//! Reload über eine Trigger-Datei + ein separates, unsandboxed
//! Path-Unit (siehe routes/mail_queue.rs,
//! config/systemd/havenmail-queue-delete.{service,path}) — kein sudo,
//! keine NoNewPrivileges-Lockerung nötig.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// `None`, wenn `postqueue` nicht ausgeführt werden konnte (z. B. lokale
/// Entwicklung ohne Postfix) — derselbe defensive Stil wie
/// `system.rs::query_unit_status`, ein fehlender Wert darf einen Snapshot
/// nicht abbrechen.
pub async fn queue_size() -> Option<usize> {
    let output = tokio::process::Command::new("postqueue")
        .arg("-p")
        .output()
        .await
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    Some(parse_queue_size(&stdout))
}

/// `postqueue -p` gibt bei leerer Queue "Mail queue is empty" aus, sonst
/// eine Tabelle gefolgt von einer Zusammenfassungszeile wie
/// "-- 3 Kbytes in 2 Requests.". Robust gegen abweichende Formatierung:
/// zählt im Zweifel die Zeilen der Tabelle selbst (jeder Eintrag beginnt
/// mit einer Queue-ID, gefolgt von Größe/Datum in derselben Zeile).
fn parse_queue_size(output: &str) -> usize {
    if output.contains("Mail queue is empty") {
        return 0;
    }
    if let Some(summary) = output.lines().find(|l| l.trim_start().starts_with("--")) {
        if let Some(count) = extract_request_count(summary) {
            return count;
        }
    }
    output
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('-') && !l.starts_with(' '))
        .filter(|l| {
            l.split_whitespace()
                .next()
                .is_some_and(|first| first.len() > 1)
        })
        .count()
        .saturating_sub(1) // Kopfzeile ("-Queue ID-  --Size--  ...")
}

fn extract_request_count(summary_line: &str) -> Option<usize> {
    let words: Vec<&str> = summary_line.split_whitespace().collect();
    let idx = words.iter().position(|w| w.starts_with("Request"))?;
    words.get(idx.checked_sub(1)?)?.parse().ok()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueRecipient {
    pub address: String,
    pub delay_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueEntry {
    pub queue_name: String,
    pub queue_id: String,
    /// Unix-Timestamp (Sekunden), so wie `postqueue -j` ihn liefert — die
    /// API wandelt das bei Bedarf in ein ISO-Datum um.
    pub arrival_time: i64,
    pub message_size: i64,
    pub sender: String,
    pub recipients: Vec<QueueRecipient>,
}

/// Listet die komplette Warteschlange (aktiv + deferred + hold) über das
/// von Postfix 3.1+ unterstützte JSON-Lines-Format (`postqueue -j`) —
/// robuster als das für Menschen gedachte Tabellenformat von `-p` zu
/// parsen.
pub async fn list_queue() -> Result<Vec<QueueEntry>, std::io::Error> {
    let output = tokio::process::Command::new("postqueue")
        .arg("-j")
        .output()
        .await?;
    Ok(parse_queue_json(&String::from_utf8_lossy(&output.stdout)))
}

fn parse_queue_json(text: &str) -> Vec<QueueEntry> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

/// Echte Postfix-Queue-IDs ("long queue ID"-Format) sind rein
/// großgeschriebene Hex-artige alphanumerische Strings. Zusammen mit dem
/// Literal `"ALL"` die einzigen Werte, die in die Trigger-Datei für
/// `havenmail-queue-delete.path` geschrieben werden dürfen — verhindert,
/// dass ein manipulierter/fehlerhafter Wert als Shell-Metazeichen im
/// root-laufenden Service-Skript landet (siehe
/// config/systemd/havenmail-queue-delete.service).
pub fn is_valid_queue_target(target: &str) -> bool {
    if target == "ALL" {
        return true;
    }
    !target.is_empty()
        && target.len() <= 20
        && target
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

/// Schreibt eine Löschanfrage für `havenmail-queue-delete.path` (root-
/// eigenes, unsandboxed systemd-Path-Unit — siehe Modul-Dokumentation
/// oben). `target` ist entweder eine einzelne Queue-ID oder `"ALL"`.
pub fn request_delete(state_dir: &Path, target: &str) -> std::io::Result<()> {
    if !is_valid_queue_target(target) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "ungültiges Ziel für Warteschlangen-Löschung",
        ));
    }
    std::fs::write(state_dir.join("queue-delete-request"), target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_queue_is_zero() {
        assert_eq!(parse_queue_size("Mail queue is empty\n"), 0);
    }

    #[test]
    fn parses_summary_line_request_count() {
        let output = "-Queue ID-  --Size-- ----Arrival Time---- -Sender/Recipient-------\n\
                       ABCD1234       522 Wed Aug  5 20:03:14  a@example.org\n\
                                                              b@example.org\n\
                       \n-- 1 Kbytes in 1 Request.\n";
        assert_eq!(parse_queue_size(output), 1);
    }

    const SAMPLE_QUEUE_JSON: &str = r#"{"queue_name": "deferred", "queue_id": "0BD9E42656", "arrival_time": 1785959240, "message_size": 522, "forced_expire": false, "sender": "a@example.org", "recipients": [{"address": "b@example.org", "delay_reason": "connect timed out"}]}
{"queue_name": "deferred", "queue_id": "83003491AA", "arrival_time": 1785959656, "message_size": 520, "forced_expire": false, "sender": "a@example.org", "recipients": [{"address": "c@example.org", "delay_reason": null}]}
"#;

    #[test]
    fn parses_live_postqueue_json_shape() {
        let entries = parse_queue_json(SAMPLE_QUEUE_JSON);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].queue_id, "0BD9E42656");
        assert_eq!(entries[0].recipients[0].address, "b@example.org");
        assert_eq!(
            entries[0].recipients[0].delay_reason.as_deref(),
            Some("connect timed out")
        );
        assert_eq!(entries[1].recipients[0].delay_reason, None);
    }

    #[test]
    fn blank_output_yields_empty_list() {
        assert_eq!(parse_queue_json("\n\n").len(), 0);
    }

    #[test]
    fn accepts_valid_queue_ids_and_all_literal() {
        assert!(is_valid_queue_target("ALL"));
        assert!(is_valid_queue_target("0BD9E42656"));
        assert!(is_valid_queue_target("83003491AA"));
    }

    #[test]
    fn rejects_shell_metacharacters_and_lowercase() {
        assert!(!is_valid_queue_target(""));
        assert!(!is_valid_queue_target("all"));
        assert!(!is_valid_queue_target("ABC; rm -rf /"));
        assert!(!is_valid_queue_target("ABC$(whoami)"));
        assert!(!is_valid_queue_target(&"A".repeat(21)));
    }
}
