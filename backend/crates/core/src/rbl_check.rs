//! Zustellbarkeits-/Reputationsprüfung: liegt die eigene öffentliche IP auf
//! einer gängigen RBL (Realtime Blackhole List)? Nutzt `hickory-resolver`
//! (dieselbe Bibliothek wie `dns_check.rs`) für die eigentliche DNS-Abfrage —
//! keine eigene DNS-Protokollimplementierung. Jede Liste wird unabhängig
//! geprüft; ein DNS-Fehler bei einer Liste (z. B. Rate-Limiting durch
//! Spamhaus bei anonymen Abfragen von Cloud-/Hosting-IPs, ein bekanntes,
//! dokumentiertes Verhalten) bedeutet "nicht prüfbar", NICHT "gelistet" —
//! ein Resolver-Hänger darf keinen Fehlalarm auslösen.

use hickory_resolver::config::{ResolverConfig, ResolverOpts};
use hickory_resolver::TokioAsyncResolver;
use serde::Serialize;
use std::net::Ipv4Addr;

/// Etablierte, kostenlos abfragbare RBLs. Absichtlich eine kleine, robuste
/// Auswahl statt vieler kleiner/unzuverlässiger Listen — mehr Listen heißt
/// mehr Rauschen (falsch-positive Listungen bei kaum genutzten RBLs sind
/// verbreitet) ohne echten Mehrwert für die Zustellbarkeitseinschätzung.
pub const KNOWN_RBLS: &[&str] = &[
    "zen.spamhaus.org",
    "bl.spamcop.net",
    "b.barracudacentral.org",
    "dnsbl.sorbs.net",
];

#[derive(Debug, Clone, Serialize)]
pub struct RblResult {
    pub zone: String,
    /// `None` = Abfrage fehlgeschlagen (Timeout, Rate-Limit, …) — kein
    /// Aussage über Listung möglich, absichtlich nicht mit "sauber"
    /// gleichgesetzt.
    pub listed: Option<bool>,
}

/// WICHTIG, live entdeckt: `ResolverConfig::default()` verwendet fest
/// einprogrammierte Google-Public-DNS-Server (8.8.8.8/8.8.4.4), NICHT den
/// vom System konfigurierten Resolver aus /etc/resolv.conf. Spamhaus (und
/// vermutlich weitere RBLs) blockt DNSBL-Abfragen über bekannte öffentliche
/// Resolver praktisch vollständig (dokumentierte Anti-Abuse-Maßnahme, siehe
/// spamhaus.org/returnc/pub/ — sie können hinter einem geteilten
/// Resolver einzelne Abfragequellen nicht unterscheiden). `dig` funktioniert
/// vom selben Host aus einwandfrei, weil es den lokalen/Provider-Resolver
/// aus /etc/resolv.conf nutzt, nicht Google — `from_system_conf()` gleicht
/// das an und liefert dieselben echten Antworten wie `dig`.
fn resolver() -> TokioAsyncResolver {
    TokioAsyncResolver::tokio_from_system_conf().unwrap_or_else(|_| {
        TokioAsyncResolver::tokio(ResolverConfig::default(), ResolverOpts::default())
    })
}

/// Kehrt die Oktette einer IPv4-Adresse um (RBL-Konvention: 1.2.3.4 wird
/// zu 4.3.2.1.<zone>).
fn reversed_octets(ip: Ipv4Addr) -> String {
    let [a, b, c, d] = ip.octets();
    format!("{d}.{c}.{b}.{a}")
}

/// Prüft eine einzelne RBL-Zone. Per RFC 5782 heißt ein A-Record unter der
/// umgekehrten IP+Zone "gelistet", konventionell aus dem Bereich
/// 127.0.0.x (Ziffer kodiert meist den Listungsgrund).
///
/// WICHTIG, live entdeckt: Spamhaus antwortet von diesem Host (typische
/// Cloud-/Hosting-IP) NICHT mit echten Listungsdaten, sondern mit
/// 127.255.255.254 — ihrem dokumentierten Fehlercode für "Abfrage
/// blockiert" (öffentliche DNSBL-Direktabfragen von bekannten Hosting-
/// Bereichen werden abgewiesen, siehe spamhaus.org/returnc/pub/). Selbst
/// der RFC-5782-Testeintrag (2.0.0.127.zen.spamhaus.org, MUSS laut RFC
/// immer als gelistet beantwortet werden) liefert denselben Fehlercode —
/// ein direkter, unverwechselbarer Beweis, dass es sich um eine
/// Fehlermeldung und keine echte Listung handelt. Nur eine Antwort mit
/// zweitem Oktett 0 (127.0.0.x) zählt hier als echte Listung; alles
/// andere (127.255.255.x u. ä.) gilt als "nicht prüfbar", nicht als
/// Fehlalarm.
pub async fn check_zone(ip: Ipv4Addr, zone: &str) -> RblResult {
    let query = format!("{}.{}", reversed_octets(ip), zone);
    // WICHTIG: `ipv4_lookup`, NICHT `lookup_ip` — Letzteres fragt per
    // Dual-Stack-Standardverhalten automatisch auch AAAA ab. RBL-Zonen
    // haben grundsätzlich nur A-Records, die AAAA-Teilabfrage liefert
    // also immer NXDOMAIN — und `lookup_ip` gibt bei einer gemischten
    // A-Erfolg/AAAA-NXDOMAIN-Antwort insgesamt einen Err zurück, der die
    // eigentliche (erfolgreiche) A-Antwort komplett verdeckt. Live real
    // aufgetreten: Spamhaus' A-Antwort 127.255.255.254 (Fehlercode, siehe
    // unten) wurde dadurch nie gesehen, jede Abfrage fiel in den
    // Err/NoRecordsFound-Zweig und meldete fälschlich "nicht gelistet".
    let listed = match resolver().ipv4_lookup(query).await {
        Ok(response) => {
            interpret_response(response.iter().next().map(|a| std::net::IpAddr::V4(a.0)))
        }
        Err(err) => {
            use hickory_resolver::error::ResolveErrorKind;
            match err.kind() {
                // NXDOMAIN heißt explizit "nicht gelistet" — ein
                // reguläres, erwartetes Ergebnis, kein Fehler.
                ResolveErrorKind::NoRecordsFound { .. } => Some(false),
                _ => None,
            }
        }
    };
    RblResult {
        zone: zone.to_string(),
        listed,
    }
}

/// Klassifiziert die erste zurückgegebene Adresse einer RBL-Antwort.
/// Eigene Funktion (statt inline in `check_zone`), damit die Spamhaus-
/// Fehlercode-Erkennung ohne echte DNS-Abfrage testbar ist.
fn interpret_response(first: Option<std::net::IpAddr>) -> Option<bool> {
    match first {
        Some(std::net::IpAddr::V4(addr)) => {
            let octets = addr.octets();
            if octets[0] == 127 && octets[1] == 0 {
                Some(true)
            } else {
                // z. B. 127.255.255.254 (Spamhaus: "Abfrage blockiert") —
                // keine echte Listungsantwort, siehe Doku oben.
                None
            }
        }
        Some(_) => None,
        None => Some(false),
    }
}

/// Prüft alle `KNOWN_RBLS` nacheinander (nur 4 Zonen, keine zusätzliche
/// Abhängigkeit für Parallelität nötig — insgesamt unter 1-2s).
pub async fn check_all(ip: Ipv4Addr) -> Vec<RblResult> {
    let mut results = Vec::with_capacity(KNOWN_RBLS.len());
    for zone in KNOWN_RBLS {
        results.push(check_zone(ip, zone).await);
    }
    results
}

/// Löst die A-Record-Adresse des eigenen Mail-Hostnamens auf (MX zeigt
/// darauf, also ist das praktisch die öffentliche Versand-IP) — kein
/// externer "was ist meine IP"-Dienst nötig, der eigene DNS-Eintrag
/// genügt.
pub async fn resolve_own_ip(mail_hostname: &str) -> Option<Ipv4Addr> {
    let response = resolver().ipv4_lookup(mail_hostname).await.ok()?;
    response.iter().next().map(|addr| addr.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reverses_octets_correctly() {
        assert_eq!(
            reversed_octets(Ipv4Addr::new(1, 2, 3, 4)),
            "4.3.2.1".to_string()
        );
    }

    #[test]
    fn conventional_listing_code_counts_as_listed() {
        let addr = std::net::IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2));
        assert_eq!(interpret_response(Some(addr)), Some(true));
    }

    #[test]
    fn spamhaus_error_code_does_not_count_as_listed() {
        // Live entdeckt: Spamhaus antwortet von Cloud-/Hosting-IPs mit
        // 127.255.255.254 ("Abfrage blockiert") statt echter Listungsdaten
        // — selbst für den RFC-5782-Testeintrag, der laut RFC IMMER als
        // gelistet beantwortet werden muss. Ohne diese Unterscheidung
        // würde jede Spamhaus-Abfrage von diesem Host als Dauer-Fehlalarm
        // "gelistet" melden.
        let addr = std::net::IpAddr::V4(Ipv4Addr::new(127, 255, 255, 254));
        assert_eq!(interpret_response(Some(addr)), None);
    }

    #[test]
    fn no_answer_means_not_listed() {
        assert_eq!(interpret_response(None), Some(false));
    }

    // KEIN Live-Test gegen echtes Spamhaus hier (bewusst): auf dem
    // Produktivserver liefert Spamhaus für die eigene IP nachweislich den
    // Fehlercode 127.255.255.254 (Anti-Missbrauchs-Blockade öffentlicher/
    // Cloud-Resolver, siehe Kommentar an `check_zone`), auf dem
    // GitHub-Actions-CI-Runner dagegen ein reguläres NXDOMAIN für dieselbe
    // IP — welche der beiden Antworten man bekommt, hängt vom Netzwerk-
    // Standort des Test-Runners ab, nicht vom Code. Ein Test, der das eine
    // konkrete Live-Verhalten fest erwartet, ist deshalb zwangsläufig
    // umgebungsabhängig flaky (real als CI-Fehlschlag aufgetreten, obwohl
    // der Code korrekt war). Die eigentliche Regressionsabsicherung -
    // "127.255.255.254 zählt nicht als Listung" - deckt die reine,
    // netzwerkfreie `interpret_response`-Testgruppe oben bereits
    // vollständig und deterministisch ab.

    #[tokio::test]
    async fn known_clean_ip_is_not_listed_on_a_real_rbl() {
        // 1.1.1.1 (Cloudflare öffentlicher Resolver) ist auf keiner
        // seriösen RBL gelistet. Barracuda statt Spamhaus, da Spamhaus
        // Direktabfragen von diesem Host nachweislich blockiert (s. o.) —
        // eine echte Netzwerkabfrage, kein Mock, analog zu den
        // bestehenden dns_check.rs-Tests gegen öffentliche Records.
        let result = check_zone(Ipv4Addr::new(1, 1, 1, 1), "b.barracudacentral.org").await;
        assert_ne!(result.listed, Some(true), "{result:?}");
    }

    #[tokio::test]
    async fn rfc5782_test_entry_is_reported_listed_where_the_rbl_supports_it() {
        // 127.0.0.2 MUSS laut RFC 5782 für jede RBL immer als gelistet
        // gemeldet werden — Barracuda hält sich live nachweislich daran
        // (Spamhaus/SORBS nicht zuverlässig von diesem Host, siehe oben).
        let result = check_zone(Ipv4Addr::new(127, 0, 0, 2), "b.barracudacentral.org").await;
        assert_eq!(result.listed, Some(true), "{result:?}");
    }
}
