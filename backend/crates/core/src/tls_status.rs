//! TLS-Ablaufstatus, gemeinsam genutzt von der System-Status-Route
//! (`api::routes::system`) und dem periodischen Benachrichtigungs-Check
//! (`havenmail-cli notify-check`) — eine Quelle der Wahrheit für die
//! Tage-bis-Ablauf-Berechnung.
//!
//! Liest NUR das Ablaufdatum, das install.sh/der certbot-Deploy-Hook nach
//! `${HAVENMAIL_ETC_DIR}/tls-expiry` schreibt (0644) — kein eigener
//! TLS-Handshake- oder Zertifikats-Parsing-Code, nur das Textformat von
//! `openssl x509 -enddate`.

use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct TlsStatus {
    /// Rohes Ablaufdatum, wie `openssl x509 -enddate` es ausgibt (z. B.
    /// "Nov  3 12:00:00 2026 GMT").
    pub expires_at: String,
    /// `None`, wenn das Datum nicht geparst werden konnte — `expires_at`
    /// wird trotzdem angezeigt, nur ohne Tage-Countdown.
    pub days_remaining: Option<i64>,
}

/// Parst das openssl-`-enddate`-Format ("Nov  3 12:00:00 2026 GMT", immer
/// GMT/UTC) zu Tagen bis zum Ablauf ab `now`. Eigene Funktion statt inline
/// in `read`, damit sie ohne Dateisystem/Env testbar ist.
pub fn days_remaining(raw_expiry: &str, now: DateTime<Utc>) -> Option<i64> {
    let without_tz = raw_expiry.trim().trim_end_matches("GMT").trim();
    let naive = chrono::NaiveDateTime::parse_from_str(without_tz, "%b %e %H:%M:%S %Y").ok()?;
    Some((naive.and_utc() - now).num_days())
}

pub fn read(etc_dir: &str) -> Option<TlsStatus> {
    let raw = std::fs::read_to_string(format!("{etc_dir}/tls-expiry")).ok()?;
    let expires_at = raw.trim().to_string();
    if expires_at.is_empty() {
        return None;
    }

    Some(TlsStatus {
        days_remaining: days_remaining(&expires_at, Utc::now()),
        expires_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn parses_openssl_enddate_format_and_computes_days_remaining() {
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let result = days_remaining("Jan 11 00:00:00 2026 GMT", now);
        assert_eq!(result, Some(10));
    }

    #[test]
    fn past_expiry_yields_negative_days() {
        let now = Utc.with_ymd_and_hms(2026, 1, 11, 0, 0, 0).unwrap();
        let result = days_remaining("Jan  1 00:00:00 2026 GMT", now);
        assert_eq!(result, Some(-10));
    }

    #[test]
    fn malformed_input_yields_none() {
        assert_eq!(days_remaining("not a date", Utc::now()), None);
    }
}
