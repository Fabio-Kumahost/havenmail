# DNS-Einrichtung

> **Status:** Der DNS-Assistent in der Web-Oberfläche (automatische Prüfung + Copy-to-Clipboard-Einträge) ist für Meilenstein M3/M4 geplant. Dieses Dokument beschreibt das Zielbild der benötigten Einträge.

Für eine Domain `example.org` mit Mail-Hostname `mail.example.org` werden benötigt:

| Typ | Name | Wert | Zweck |
|---|---|---|---|
| A / AAAA | `mail.example.org` | IP des Servers | Adressierung des Mail-Hosts |
| MX | `example.org` | `10 mail.example.org.` | Mailzustellung |
| PTR | (Reverse-Zone des Providers) | `mail.example.org.` | Reverse-DNS, kritisch für Zustellbarkeit |
| TXT (SPF) | `example.org` | `v=spf1 mx -all` | Autorisierte Versender |
| TXT (DKIM) | `<selector>._domainkey.example.org` | von Havenmail generierter Public Key | Signaturprüfung |
| TXT (DMARC) | `_dmarc.example.org` | `v=DMARC1; p=quarantine; rua=mailto:dmarc@example.org` | Richtlinie bei SPF/DKIM-Fehlern |
| TXT (MTA-STS) | `_mta-sts.example.org` | `v=STSv1; id=<timestamp>` | TLS-Erzwingung ankündigen |
| HTTPS | `mta-sts.example.org` | Policy-Datei über HTTPS | MTA-STS-Policy |
| TXT (TLS-RPT) | `_smtp._tls.example.org` | `v=TLSRPTv1; rua=mailto:tlsrpt@example.org` | TLS-Fehlerberichte |
| TLSA (optional, bei DNSSEC) | `_25._tcp.mail.example.org` | Hash des TLS-Zertifikats | DANE |

Der geplante DNS-Assistent zeigt diese Einträge mit den tatsächlichen, für die jeweilige Installation generierten Werten an (inkl. Kopier-Button) und prüft sie live per DNS-Abfrage.
