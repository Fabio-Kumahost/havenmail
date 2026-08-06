//! Tatsächliche Festplattennutzung einer Mailbox (Admin-Panel,
//! Benutzerliste einer Domain) — im Gegensatz zu `quota_bytes`
//! (konfiguriertes Limit) die reale Belegung, für Kapazitätsplanung.
//!
//! `du -sb` statt eigener Verzeichnis-Traversierung: die Maildir-Struktur
//! (new/cur/tmp, potenziell mit Sieve-Skripten, Index-Dateien) exakt
//! nachzubilden wäre fehleranfällig: etablisches Tool statt Neuerfindung.

use std::path::Path;

/// `None` bei jedem Fehler (Mailbox existiert noch nicht, `du` nicht
/// ausführbar o. Ä.) — defensiv wie alle anderen Sammler, siehe
/// `system.rs::query_unit_status`.
pub async fn usage_bytes(path: &Path) -> Option<i64> {
    let output = tokio::process::Command::new("du")
        .args(["-sb", "--"])
        .arg(path)
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_du_output(&String::from_utf8_lossy(&output.stdout))
}

/// `du -sb <path>` gibt "<bytes>\t<path>" aus.
fn parse_du_output(output: &str) -> Option<i64> {
    output.split_whitespace().next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tab_separated_du_output() {
        assert_eq!(
            parse_du_output("44725\t/var/mail/havenmail/x/y\n"),
            Some(44725)
        );
    }

    #[test]
    fn malformed_output_yields_none() {
        assert_eq!(parse_du_output(""), None);
        assert_eq!(parse_du_output("not a number"), None);
    }
}
