//! DNS-Einrichtungsprüfung (MX, SPF, DKIM, DMARC).
//!
//! Nutzt `hickory-resolver` (früher trust-dns, aktiv gepflegt) für alle
//! DNS-Abfragen — keine eigene DNS-Protokollimplementierung. Ergebnisse
//! werden vom Aufrufer in `dns_checks` persistiert (siehe
//! `backend/migrations/0001_core_schema.sql`).

use hickory_resolver::config::{ResolverConfig, ResolverOpts};
use hickory_resolver::TokioAsyncResolver;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DnsCheckError {
    #[error("DNS-Abfrage fehlgeschlagen: {0}")]
    Lookup(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Ok,
    Missing,
    Mismatch,
}

#[derive(Debug, Clone, Serialize)]
pub struct DnsCheckResult {
    pub record_type: String,
    pub expected: String,
    pub actual: Option<String>,
    pub status: CheckStatus,
}

fn resolver() -> TokioAsyncResolver {
    TokioAsyncResolver::tokio(ResolverConfig::default(), ResolverOpts::default())
}

/// Prüft, ob unter `mx_hostname` ein passender MX-Eintrag für `domain` existiert.
pub async fn check_mx(domain: &str, expected_hostname: &str) -> DnsCheckResult {
    let resolver = resolver();
    match resolver.mx_lookup(format!("{domain}.")).await {
        Ok(lookup) => {
            let hosts: Vec<String> = lookup
                .iter()
                .map(|mx| mx.exchange().to_string().trim_end_matches('.').to_string())
                .collect();
            let expected_norm = expected_hostname.trim_end_matches('.');
            if hosts.iter().any(|h| h.eq_ignore_ascii_case(expected_norm)) {
                DnsCheckResult {
                    record_type: "MX".to_string(),
                    expected: expected_hostname.to_string(),
                    actual: Some(hosts.join(", ")),
                    status: CheckStatus::Ok,
                }
            } else {
                DnsCheckResult {
                    record_type: "MX".to_string(),
                    expected: expected_hostname.to_string(),
                    actual: Some(hosts.join(", ")),
                    status: CheckStatus::Mismatch,
                }
            }
        }
        Err(_) => DnsCheckResult {
            record_type: "MX".to_string(),
            expected: expected_hostname.to_string(),
            actual: None,
            status: CheckStatus::Missing,
        },
    }
}

/// Prüft einen beliebigen TXT-Record (z. B. SPF an `domain`, DKIM an
/// `<selector>._domainkey.<domain>`, DMARC an `_dmarc.<domain>`) auf einen
/// Teilstring-Treffer (SPF/DMARC/DKIM-Records enthalten oft zusätzliche,
/// unkritische Bestandteile, daher `contains` statt exakter Gleichheit).
pub async fn check_txt_contains(
    record_type: &str,
    fqdn: &str,
    expected_substring: &str,
) -> DnsCheckResult {
    let resolver = resolver();
    match resolver.txt_lookup(format!("{fqdn}.")).await {
        Ok(lookup) => {
            let values: Vec<String> = lookup.iter().map(|txt| txt.to_string()).collect();
            let found = values.iter().any(|v| v.contains(expected_substring));
            DnsCheckResult {
                record_type: record_type.to_string(),
                expected: expected_substring.to_string(),
                actual: Some(values.join(" | ")),
                status: if found {
                    CheckStatus::Ok
                } else {
                    CheckStatus::Mismatch
                },
            }
        }
        Err(_) => DnsCheckResult {
            record_type: record_type.to_string(),
            expected: expected_substring.to_string(),
            actual: None,
            status: CheckStatus::Missing,
        },
    }
}

/// Erwartete DNS-Einträge für eine Domain, wie sie in der Web-UI/CLI zum
/// Kopieren angezeigt werden (siehe docs/dns-setup.md). Rein informativ,
/// keine DNS-Abfrage.
#[derive(Debug, Clone, Serialize)]
pub struct RecommendedDnsRecords {
    pub mx: String,
    pub spf: String,
    pub dkim_selector: String,
    pub dkim_value: String,
    pub dmarc: String,
}

pub fn recommend_dns_records(
    mail_hostname: &str,
    dkim_selector: &str,
    dkim_dns_value: &str,
    dmarc_report_address: &str,
) -> RecommendedDnsRecords {
    RecommendedDnsRecords {
        mx: format!("10 {mail_hostname}."),
        spf: "v=spf1 mx -all".to_string(),
        dkim_selector: format!("{dkim_selector}._domainkey"),
        dkim_value: dkim_dns_value.to_string(),
        dmarc: format!("v=DMARC1; p=quarantine; rua=mailto:{dmarc_report_address}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recommend_dns_records_produces_expected_shape() {
        let rec = recommend_dns_records(
            "mail.example.org",
            "havenmail",
            "v=DKIM1; k=rsa; p=AAAA",
            "dmarc@example.org",
        );
        assert_eq!(rec.mx, "10 mail.example.org.");
        assert_eq!(rec.spf, "v=spf1 mx -all");
        assert_eq!(rec.dkim_selector, "havenmail._domainkey");
        assert!(rec.dmarc.contains("rua=mailto:dmarc@example.org"));
    }

    /// Diese Tests führen eine echte DNS-Abfrage gegen öffentliche Resolver
    /// aus und werden nur ausgeführt, wenn Netzwerkzugriff erlaubt ist
    /// (Kennzeichnung über `HAVENMAIL_TEST_NETWORK=1`), damit `cargo test`
    /// standardmäßig ohne Internetzugriff funktioniert.
    fn network_tests_enabled() -> bool {
        std::env::var("HAVENMAIL_TEST_NETWORK").as_deref() == Ok("1")
    }

    #[tokio::test]
    async fn check_mx_finds_known_public_record() {
        if !network_tests_enabled() {
            eprintln!("HAVENMAIL_TEST_NETWORK != 1 — Netzwerktest übersprungen");
            return;
        }
        // example.com hat historisch stabile, öffentlich dokumentierte DNS-Daten.
        let result = check_mx("example.com", "definitely-wrong-mx.invalid").await;
        assert_eq!(result.status, CheckStatus::Mismatch);
    }
}
