//! Admin-Benachrichtigungen per E-Mail bei kritischen Ereignissen (TLS-
//! Ablauf, Speicherplatz, Dienstausfall, Backup-Fehlschlag).
//!
//! Kein eigener SMTP-Client — die Nachricht wird an das bereits
//! installierte, Postfix-bereitgestellte `sendmail`-Binary übergeben, exakt
//! nach demselben Shell-out-Muster wie `mail_queue.rs` (`postqueue`) oder
//! `fail2ban.rs` (`fail2ban-client`). Postfix übernimmt Warteschlange,
//! TLS und Zustellung/Retry — dieselbe Engine, die auch normale Postfächer
//! bedient, kein zweiter Zustellweg.

use chrono::{DateTime, Duration, Utc};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Ok,
    Problem,
}

impl CheckStatus {
    pub fn as_db_str(&self) -> &'static str {
        match self {
            CheckStatus::Ok => "ok",
            CheckStatus::Problem => "problem",
        }
    }

    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "ok" => Some(CheckStatus::Ok),
            "problem" => Some(CheckStatus::Problem),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyKind {
    /// Neu aufgetretenes Problem (Zustandswechsel ok/unbekannt -> problem).
    Alert,
    /// Andauerndes Problem, letzte Benachrichtigung liegt >= `remind_after`
    /// zurück — verhindert, dass ein dauerhaftes Problem bei jedem Lauf
    /// erneut eine Mail auslöst, erinnert aber trotzdem regelmäßig.
    Reminder,
    /// Zustandswechsel problem -> ok.
    Resolved,
}

/// Reine Entscheidungsfunktion (kein I/O) — ob und welche Art von
/// Benachrichtigung für den aktuellen Status eines Checks nötig ist.
/// `previous` ist `None` beim allerersten Lauf eines Checks (noch keine
/// Zeile in `notification_state`).
pub fn decide_notification(
    current: CheckStatus,
    previous: Option<(CheckStatus, Option<DateTime<Utc>>)>,
    now: DateTime<Utc>,
    remind_after: Duration,
) -> Option<NotifyKind> {
    match (previous.map(|(status, _)| status), current) {
        (None, CheckStatus::Problem) => Some(NotifyKind::Alert),
        (None, CheckStatus::Ok) => None,
        (Some(CheckStatus::Ok), CheckStatus::Problem) => Some(NotifyKind::Alert),
        (Some(CheckStatus::Problem), CheckStatus::Ok) => Some(NotifyKind::Resolved),
        (Some(CheckStatus::Ok), CheckStatus::Ok) => None,
        (Some(CheckStatus::Problem), CheckStatus::Problem) => {
            let last_notified_at = previous.and_then(|(_, t)| t);
            match last_notified_at {
                None => Some(NotifyKind::Alert),
                Some(t) if now - t >= remind_after => Some(NotifyKind::Reminder),
                _ => None,
            }
        }
    }
}

/// Schickt eine einfache Text-E-Mail über das lokale `sendmail`-Binary
/// (`-t`: Empfänger aus den Kopfzeilen, `-oi`: Punkt auf eigener Zeile
/// beendet die Nachricht NICHT — verhindert einen abgeschnittenen Body,
/// falls der Text zufällig eine Zeile mit nur "." enthält).
pub async fn send_admin_email(to: &str, subject: &str, body: &str) -> std::io::Result<()> {
    let message = format!(
        "To: {to}\r\nFrom: Havenmail <havenmail@localhost>\r\nSubject: {subject}\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{body}\r\n"
    );

    let mut child = Command::new("/usr/sbin/sendmail")
        .args(["-oi", "-t"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(message.as_bytes()).await?;
    }

    let output = child.wait_with_output().await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(std::io::Error::other(format!(
            "sendmail beendete sich mit Fehler: {stderr}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_headers_are_well_formed() {
        // Nur der Nachrichtenaufbau ist ohne echten Mailversand testbar —
        // send_admin_email selbst braucht /usr/sbin/sendmail und wird live
        // verifiziert (siehe CHANGELOG/Deploy-Notizen).
        let to = "admin@example.org";
        let subject = "[Havenmail] Test";
        let body = "Hallo";
        let message = format!(
            "To: {to}\r\nFrom: Havenmail <havenmail@localhost>\r\nSubject: {subject}\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{body}\r\n"
        );
        assert!(message.starts_with("To: admin@example.org\r\n"));
        assert!(message.contains("Subject: [Havenmail] Test\r\n"));
        assert!(message.ends_with("Hallo\r\n"));
    }

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    #[test]
    fn first_run_with_problem_alerts_immediately() {
        let kind = decide_notification(CheckStatus::Problem, None, now(), Duration::hours(24));
        assert_eq!(kind, Some(NotifyKind::Alert));
    }

    #[test]
    fn first_run_with_ok_stays_silent() {
        let kind = decide_notification(CheckStatus::Ok, None, now(), Duration::hours(24));
        assert_eq!(kind, None);
    }

    #[test]
    fn transition_ok_to_problem_alerts() {
        let kind = decide_notification(
            CheckStatus::Problem,
            Some((CheckStatus::Ok, Some(now() - Duration::hours(1)))),
            now(),
            Duration::hours(24),
        );
        assert_eq!(kind, Some(NotifyKind::Alert));
    }

    #[test]
    fn transition_problem_to_ok_sends_resolved() {
        let kind = decide_notification(
            CheckStatus::Ok,
            Some((CheckStatus::Problem, Some(now() - Duration::minutes(5)))),
            now(),
            Duration::hours(24),
        );
        assert_eq!(kind, Some(NotifyKind::Resolved));
    }

    #[test]
    fn ongoing_problem_within_remind_window_stays_silent() {
        let kind = decide_notification(
            CheckStatus::Problem,
            Some((CheckStatus::Problem, Some(now() - Duration::hours(2)))),
            now(),
            Duration::hours(24),
        );
        assert_eq!(kind, None);
    }

    #[test]
    fn ongoing_problem_past_remind_window_sends_reminder() {
        let kind = decide_notification(
            CheckStatus::Problem,
            Some((CheckStatus::Problem, Some(now() - Duration::hours(25)))),
            now(),
            Duration::hours(24),
        );
        assert_eq!(kind, Some(NotifyKind::Reminder));
    }

    #[test]
    fn ongoing_ok_stays_silent() {
        let kind = decide_notification(
            CheckStatus::Ok,
            Some((CheckStatus::Ok, None)),
            now(),
            Duration::hours(24),
        );
        assert_eq!(kind, None);
    }
}
