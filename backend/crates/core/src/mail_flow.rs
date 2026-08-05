//! Zählt gesendete (ausgehend, `postfix/smtp`) und empfangene (lokal
//! zugestellt, `postfix/virtual`) Mail seit einem Zeitpunkt — für den
//! Dashboard-Chart "Gesendet vs. Empfangen". Kein separates `mail.log`
//! auf diesem System (Postfix loggt per Default über journald), daher
//! über `journalctl -u postfix --since` statt Datei-Parsing wie bei
//! `clamav_stats`.

use chrono::{DateTime, Utc};

/// `(0, 0)` bei jedem Fehler (journalctl nicht ausführbar o. Ä.) — wie
/// überall in den Metrik-Sammlern lieber ein leerer Wert als ein
/// abgebrochener Snapshot, siehe `system.rs::query_unit_status`.
pub async fn counts_since(since: DateTime<Utc>) -> (usize, usize) {
    let output = tokio::process::Command::new("journalctl")
        .args([
            "-u",
            "postfix",
            "--since",
            &since.format("%Y-%m-%d %H:%M:%S").to_string(),
            "--no-pager",
        ])
        .output()
        .await;
    match output {
        Ok(output) => parse_flow_counts(&String::from_utf8_lossy(&output.stdout)),
        Err(_) => (0, 0),
    }
}

/// `postfix/smtp[PID]: ... status=sent` = ausgehend an einen externen
/// Relay zugestellt. `postfix/virtual[PID]: ... status=sent` = lokal in
/// ein Postfach zugestellt (siehe `virtual_transport`, das Postfach-
/// Modell dieser Installation nutzt ausschließlich `virtual`, kein
/// `lmtp`). Beide Präfixe live gegen diese Installation verifiziert.
fn parse_flow_counts(log_text: &str) -> (usize, usize) {
    let mut sent = 0;
    let mut received = 0;
    for line in log_text.lines() {
        if !line.contains("status=sent") {
            continue;
        }
        if line.contains("postfix/smtp[") {
            sent += 1;
        } else if line.contains("postfix/virtual[") {
            received += 1;
        }
    }
    (sent, received)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_LOG: &str = "\
Aug 05 19:59:22 host postfix/smtp[51198]: C81C749190: to=<a@gmail.com>, relay=gmail-smtp-in..., status=sent (250 2.0.0 OK)
Aug 05 20:03:14 host postfix/virtual[51484]: 7BEF249190: to=<fabio@xfabio.de>, relay=virtual, status=sent (delivered to maildir)
Aug 05 20:03:19 host postfix/submission/smtpd[51492]: connect from localhost[::1]
Aug 05 20:03:21 host postfix/smtp[51335]: C5DE149190: to=<b@gmail.com>, relay=gmail-smtp-in..., status=sent (250 2.0.0 OK)
Aug 05 20:08:12 host postfix/virtual[51790]: 5B88C49181: to=<fabio@xfabio.de>, relay=virtual, status=sent (delivered to maildir)
Aug 05 20:08:12 host postfix/virtual[51790]: XYZ: to=<fabio@xfabio.de>, status=deferred (mailbox full)
";

    #[test]
    fn counts_sent_and_received_separately() {
        let (sent, received) = parse_flow_counts(SAMPLE_LOG);
        assert_eq!(sent, 2);
        assert_eq!(received, 2);
    }

    #[test]
    fn empty_log_yields_zero() {
        assert_eq!(parse_flow_counts(""), (0, 0));
    }

    #[test]
    fn ignores_non_sent_status_lines() {
        let (sent, received) = parse_flow_counts(
            "Aug 05 20:08:12 host postfix/virtual[1]: X: status=deferred (mailbox full)\n",
        );
        assert_eq!((sent, received), (0, 0));
    }
}
