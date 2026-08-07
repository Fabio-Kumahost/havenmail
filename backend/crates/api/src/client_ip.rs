//! Client-IP-Ermittlung für Rate-Limiting/Audit-Logging.
//!
//! Havenmail läuft laut Architektur (docs/architecture.md, Portübersicht)
//! immer hinter einem Reverse-Proxy (nginx/caddy) auf demselben Host, der
//! den API-Prozess ausschließlich über `127.0.0.1` erreicht — eine direkte
//! TCP-Peer-Adresse wäre daher immer die Proxy-Loopback-Adresse, nicht die
//! des tatsächlichen Clients. Stattdessen wird die vom Proxy gesetzte
//! `X-Forwarded-For`-Kopfzeile ausgewertet (erster Eintrag = ursprünglicher
//! Client). Fehlt der Header (z. B. Fehlkonfiguration), wird eine
//! unspezifizierte Adresse verwendet — das Rate-Limiting bündelt dann zwar
//! alle Clients, verhindert aber nicht wenigstens den einfachsten Fall.
//!
//! SICHERHEITSANNAHME: `extract()` nimmt den ERSTEN Eintrag als Client-IP
//! — das ist nur sicher, weil `config/nginx/havenmail.conf.tera` den
//! `X-Forwarded-For`-Header beim Weiterleiten an die API bewusst mit
//! `$remote_addr` ÜBERSCHREIBT statt (wie zuvor, per
//! `$proxy_add_x_forwarded_for`) nur einen vom Client mitgeschickten Wert
//! ANZUHÄNGEN. Mit der alten Append-Logik konnte ein Client einen
//! beliebigen `X-Forwarded-For`-Wert selbst mitschicken und damit sowohl
//! den Login-Rate-Limiter (`rate_limit.rs`) als auch die in `audit_log`
//! gespeicherte IP frei fälschen (gefunden im Sicherheits-/Bug-Audit vom
//! 2026-08-07) — nginx ist laut Architektur der einzige Hop vor der API,
//! ein zusätzlicher, echter Reverse-Proxy davor (CDN o. Ä.) ist in dieser
//! Topologie nicht vorgesehen. Wird diese Topologie je erweitert (z. B.
//! ein CDN vor nginx), muss diese Annahme neu geprüft werden.

use axum::http::HeaderMap;
use std::net::IpAddr;

pub fn extract(headers: &HeaderMap) -> IpAddr {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(str::trim)
        .and_then(|s| s.parse().ok())
        .unwrap_or(IpAddr::from([0, 0, 0, 0]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn extracts_first_ip_from_forwarded_chain() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("203.0.113.9, 10.0.0.1"),
        );
        assert_eq!(extract(&headers), "203.0.113.9".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn falls_back_to_unspecified_when_missing() {
        let headers = HeaderMap::new();
        assert_eq!(extract(&headers), IpAddr::from([0, 0, 0, 0]));
    }

    #[test]
    fn falls_back_when_header_is_malformed() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("not-an-ip"));
        assert_eq!(extract(&headers), IpAddr::from([0, 0, 0, 0]));
    }
}
