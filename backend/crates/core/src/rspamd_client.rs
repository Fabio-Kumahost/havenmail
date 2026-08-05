//! Minimaler HTTP-Client für den Rspamd-Controller (nur Stats lesen — das
//! `/stat`-JSON ist live gegen diese Installation verifiziert).
//!
//! Der Controller läuft per Debian-Paket-Default nur auf `127.0.0.1:11334`
//! mit `secure_ip = 127.0.0.1` (worker-controller.inc, nicht von Havenmail
//! templated) — Requests vom selben Host sind bereits privilegiert, es ist
//! bewusst kein API-Passwort verdrahtet. Läuft die Control-Plane-API je
//! auf einem anderen Host als Rspamd, müsste das nachgezogen werden.
//!
//! Kein Reload-Endpunkt: Rspamds Controller-API bietet trotz ursprünglicher
//! Annahme keinen HTTP-Weg, die Konfiguration neu zu laden (live geprüft:
//! weder `/reload` noch vergleichbare Pfade existieren, auch `rspamc`
//! kennt kein solches Kommando) — Config-Reload läuft stattdessen wie
//! überall sonst im Projekt über `systemctl reload rspamd`, siehe
//! routes/security_settings.rs.

use serde::Deserialize;
use thiserror::Error;

const DEFAULT_BASE_URL: &str = "http://127.0.0.1:11334";

#[derive(Debug, Error)]
pub enum RspamdError {
    #[error("Rspamd-Controller nicht erreichbar: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Rspamd-Controller antwortete mit Status {0}")]
    Status(reqwest::StatusCode),
}

#[derive(Debug, Clone, Deserialize)]
pub struct RspamdActions {
    pub reject: u64,
    #[serde(rename = "soft reject")]
    pub soft_reject: u64,
    #[serde(rename = "rewrite subject")]
    pub rewrite_subject: u64,
    #[serde(rename = "add header")]
    pub add_header: u64,
    pub greylist: u64,
    #[serde(rename = "no action")]
    pub no_action: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RspamdStat {
    pub scanned: u64,
    pub spam_count: u64,
    pub ham_count: u64,
    pub actions: RspamdActions,
}

pub struct RspamdClient {
    base_url: String,
    http: reqwest::Client,
}

impl Default for RspamdClient {
    /// Basis-URL per `RSPAMD_CONTROLLER_URL` überschreibbar (Tests, oder
    /// falls Rspamd künftig auf einem anderen Host läuft).
    fn default() -> Self {
        let base_url = std::env::var("RSPAMD_CONTROLLER_URL")
            .unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
        Self {
            base_url,
            http: reqwest::Client::new(),
        }
    }
}

impl RspamdClient {
    /// Kumulative Zähler seit Rspamd-Prozessstart — kein Zeitverlauf. Für
    /// Trendcharts bildet der Aufrufer Deltas zwischen periodischen
    /// Snapshots (siehe `havenmail-cli snapshot-metrics`).
    pub async fn stat(&self) -> Result<RspamdStat, RspamdError> {
        let res = self
            .http
            .get(format!("{}/stat", self.base_url))
            .send()
            .await?;
        if !res.status().is_success() {
            return Err(RspamdError::Status(res.status()));
        }
        Ok(res.json::<RspamdStat>().await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixture: exakte Feldform, live gegen den Rspamd-Controller dieser
    /// Installation verifiziert (rspamd 3.12.1).
    const SAMPLE_STAT_JSON: &str = r#"{
        "version": "3.12.1",
        "scanned": 13,
        "actions": {
            "reject": 0,
            "soft reject": 0,
            "rewrite subject": 0,
            "add header": 0,
            "greylist": 1,
            "no action": 12
        },
        "spam_count": 0,
        "ham_count": 13
    }"#;

    #[test]
    fn parses_live_rspamd_stat_shape() {
        let stat: RspamdStat = serde_json::from_str(SAMPLE_STAT_JSON).expect("sollte parsen");
        assert_eq!(stat.scanned, 13);
        assert_eq!(stat.ham_count, 13);
        assert_eq!(stat.actions.greylist, 1);
        assert_eq!(stat.actions.no_action, 12);
    }
}
