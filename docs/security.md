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

## Implementierungsstand (M1)

- **Passwort-Hashing:** Argon2id über die `argon2`-Crate (`backend/crates/core/src/auth/password.rs`), Standardparameter der Bibliothek, kein Eigenbau.
- **Access-Tokens:** HS256-JWTs mit 15 Minuten Gültigkeit (`backend/crates/core/src/auth/jwt.rs`).
- **Refresh-Sessions/API-Keys:** zufällige 256-Bit-Opak-Tokens, nur als SHA-256-Hash gespeichert, jederzeit widerrufbar (`backend/crates/core/src/auth/token.rs`).
- **2FA:** RFC-6238-TOTP über die `totp-rs`-Crate (`backend/crates/core/src/auth/totp.rs`).
- **RBAC:** `backend/crates/core/src/rbac.rs` — jede Aktion wird gegen Rolle *und* Domain-Scope geprüft; ein `domain_admin`-Token ohne `domain_id` wird als „keine Berechtigung“ behandelt, nicht als Fehler ignoriert.
- **Audit-Log:** Hash-Chain-Verkettung (`backend/crates/core/src/audit.rs`) — nachträgliches Ändern oder Entfernen eines Eintrags aus der Mitte der Kette wird bei `verify_chain` erkannt.
- **Mail-Zustellung ohne Open Relay:** `config/postfix/main.cf.tera` erzwingt `reject_unauth_destination` und SASL-Pflicht für Relay; Submission/SMTPS verlangen Authentifizierung (`config/postfix/master.cf.append.tera`).

Alle genannten Module sind mit Unit-Tests abgedeckt (`cargo test -p havenmail-core`); die Datenbankmigrationen sind zusätzlich gegen eine echte PostgreSQL-Instanz verifiziert (CI: `.github/workflows/backend.yml`).

Dieser Bereich wird mit ACME/DKIM-Schlüsselerzeugung und SPF/DMARC/DNS-Prüfungen (M3) um weitere konkrete Konfigurationsbeispiele erweitert.
