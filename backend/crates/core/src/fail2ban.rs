//! Fail2Ban-Status auslesen/entsperren (Admin-Panel "Fail2Ban"-Seite).
//!
//! Der Steuer-Socket ist root:root 0600 (`fail2ban-client status ...`
//! braucht den Socket, live geprüft: "Permission denied" für den
//! `havenmail`-Systembenutzer). `havenmail-cli` läuft für diese Befehle
//! als root über eigene, unsandboxed systemd-Units (siehe
//! config/systemd/havenmail-fail2ban-*.{service,path,timer}) — nicht die
//! gehärtete havenmail-api. Die API selbst liest nur die hier
//! geschriebene JSON-Statusdatei, kein direkter fail2ban-Zugriff.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Jails, die Havenmail selbst ausliefert (siehe
/// config/fail2ban/havenmail-{postfix,dovecot}.conf.tera) plus das
/// Debian-Standard-`sshd`-Jail — bewusst eine feste Liste statt "alle
/// Jails aus fail2ban-client status" zu übernehmen, damit ein Admin, der
/// zusätzliche eigene Jails anlegt, diese nicht versehentlich über das
/// Havenmail-Panel fernsteuerbar macht.
pub const MANAGED_JAILS: &[&str] = &["sshd", "postfix-sasl", "dovecot"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JailStatus {
    pub name: String,
    pub banned: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fail2banStatus {
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub jails: Vec<JailStatus>,
}

/// Fragt alle `MANAGED_JAILS` ab und schreibt das Ergebnis als JSON nach
/// `status_file` (0644 — enthält nur IP-Adressen, keine Geheimnisse).
/// Ein einzelnes nicht (mehr) existierendes Jail überspringt lediglich
/// diesen Eintrag, statt den gesamten Refresh abzubrechen.
pub async fn refresh_status(status_file: &Path) -> std::io::Result<Fail2banStatus> {
    let mut jails = Vec::new();
    for &jail in MANAGED_JAILS {
        let output = tokio::process::Command::new("fail2ban-client")
            .args(["status", jail])
            .output()
            .await?;
        if !output.status.success() {
            continue;
        }
        let banned = parse_banned_ips(&String::from_utf8_lossy(&output.stdout));
        jails.push(JailStatus {
            name: jail.to_string(),
            banned,
        });
    }
    let status = Fail2banStatus {
        updated_at: chrono::Utc::now(),
        jails,
    };
    std::fs::write(status_file, serde_json::to_string_pretty(&status)?)?;
    Ok(status)
}

/// `fail2ban-client status <jail>` gibt u. a. eine Zeile
/// "`   \`- Banned IP list:\tIP1 IP2 IP3`" aus (Leerstring nach dem
/// Doppelpunkt, wenn niemand gesperrt ist).
fn parse_banned_ips(status_text: &str) -> Vec<String> {
    status_text
        .lines()
        .find(|l| l.contains("Banned IP list:"))
        .and_then(|l| l.split("Banned IP list:").nth(1))
        .map(|rest| rest.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default()
}

pub fn is_valid_jail(jail: &str) -> bool {
    MANAGED_JAILS.contains(&jail)
}

/// Entsperrt `ip` in `jail`. `false` (statt Fehler) bei ungültigem
/// Jail-Namen oder ungültiger IP — der Aufrufer (CLI) soll daraus einen
/// klaren Fehler machen, ohne dass ein manipulierter Trigger-Datei-Inhalt
/// je an `fail2ban-client` als Shell-Argument durchgereicht wird, ohne
/// vorher validiert zu sein.
pub async fn unban(jail: &str, ip: &str) -> std::io::Result<bool> {
    if !is_valid_jail(jail) || ip.parse::<std::net::IpAddr>().is_err() {
        return Ok(false);
    }
    let output = tokio::process::Command::new("fail2ban-client")
        .args(["set", jail, "unbanip", ip])
        .output()
        .await?;
    Ok(output.status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_STATUS: &str = "Status for the jail: sshd\n\
|- Filter\n\
|  |- Currently failed:\t0\n\
|  |- Total failed:\t0\n\
|  `- Journal matches:\t_SYSTEMD_UNIT=ssh.service + _COMM=sshd\n\
`- Actions\n\
   |- Currently banned:\t2\n\
   |- Total banned:\t9\n\
   `- Banned IP list:\t1.2.3.4 5.6.7.8\n";

    #[test]
    fn parses_banned_ip_list() {
        assert_eq!(parse_banned_ips(SAMPLE_STATUS), vec!["1.2.3.4", "5.6.7.8"]);
    }

    #[test]
    fn empty_ban_list_yields_empty_vec() {
        let text = "   `- Banned IP list:\t\n";
        assert_eq!(parse_banned_ips(text), Vec::<String>::new());
    }

    #[test]
    fn missing_line_yields_empty_vec() {
        assert_eq!(parse_banned_ips("no such line here"), Vec::<String>::new());
    }

    #[test]
    fn rejects_unmanaged_jail_names() {
        assert!(is_valid_jail("sshd"));
        assert!(!is_valid_jail("some-other-jail"));
        assert!(!is_valid_jail("sshd; rm -rf /"));
    }
}
