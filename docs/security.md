# Sicherheitsmodell

Das vollständige Bedrohungsmodell, die Gegenmaßnahmen und das Auth-/RBAC-Design stehen in [architecture.md](architecture.md#bedrohungsanalyse-kurzfassung) und [architecture.md](architecture.md#sicherheitsmodell).

## Kurzfassung der Default-Sicherheitsprinzipien

- Kein Open Relay: SASL-Auth-Pflicht für ausgehenden Versand, restriktive `smtpd_relay_restrictions`
- TLS 1.2+ (Default 1.3) erzwungen auf allen extern erreichbaren Diensten
- Argon2id für alle Passwort-Hashes (Admin-UI wie Mail-Auth)
- RBAC mit serverseitiger Durchsetzung des Domain-Scopes
- Audit-Log für alle administrativen Änderungen (append-only, Hash-Chain)
- Keine Klartext-Zugangsdaten in Logs oder Mails
- Sicherheitsrelevante Meldungen: siehe [SECURITY.md](../SECURITY.md)

Dieser Bereich wird mit der Implementierung von Auth/RBAC (M1) und ACME/DKIM/DNS-Prüfungen (M3) um konkrete Konfigurationsbeispiele erweitert.
