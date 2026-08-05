//! Größe der Postfix-Warteschlange über `postqueue -p` (kein direkter
//! Zugriff auf `/var/spool/postfix`, das der `havenmail`-Systembenutzer
//! nicht lesen darf — `postqueue` läuft setgid `postdrop`).

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
        .filter(|l| l.split_whitespace().next().is_some_and(|first| first.len() > 1))
        .count()
        .saturating_sub(1) // Kopfzeile ("-Queue ID-  --Size--  ...")
}

fn extract_request_count(summary_line: &str) -> Option<usize> {
    let words: Vec<&str> = summary_line.split_whitespace().collect();
    let idx = words.iter().position(|w| w.starts_with("Request"))?;
    words.get(idx.checked_sub(1)?)?.parse().ok()
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
}
