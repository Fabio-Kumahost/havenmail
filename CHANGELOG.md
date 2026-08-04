# Changelog

Format angelehnt an [Keep a Changelog](https://keepachangelog.com/de/1.0.0/), Versionierung nach [SemVer](https://semver.org/lang/de/).

## [Unreleased]

### Hinzugefügt (M1: sicherer Kern)
- Kern-Datenmodell als sqlx-Migrationen (`backend/migrations/0001_core_schema.sql`): Domains, Benutzer, Aliase, Verteiler, Weiterleitungen, API-Tokens, Sessions, DKIM-Schlüssel, Audit-Log, Backup-Runs, DNS-Checks
- SQL-Views für Postfix-/Dovecot-Lookups (`0002_mail_lookup_views.sql`): virtuelle Domains/Mailboxen/Aliase inkl. Catch-all und Verteiler, Dovecot-SASL-Auth-View — Postfix/Dovecot fragen diese direkt und read-only ab, kein eigener Vermittlungscode
- Neue Crate `havenmail-core`: Argon2id-Passwort-Hashing, HS256-JWT-Access-Tokens, widerrufbare Opak-Tokens (Refresh-Sessions/API-Keys), RFC-6238-TOTP-2FA, RBAC (super_admin/domain_admin/user) mit serverseitiger Scope-Prüfung, Audit-Log mit Hash-Chain-Verifikation, Tera-basiertes Config-Rendering
- Postfix-/Dovecot-/Rspamd-Konfigurationstemplates (`config/`): Open-Relay-Schutz, SASL-Auth über Dovecot, TLS-Erzwingung, Rate-Limiting, DKIM-Signing- und ClamAV-Antivirus-Anbindung über Rspamd
- Control-Plane-API führt Migrationen beim Start aus und prüft die DB-Verbindung im `/readyz`-Endpunkt
- CI: Backend-Workflow startet einen Postgres-16-Service und führt die echten Migrationstests aus

### Hinzugefügt (M0)
- Repository-Grundgerüst: README, LICENSE (AGPL-3.0-or-later), SECURITY.md, CONTRIBUTING.md
- Architekturdokumentation (`docs/architecture.md`) mit Bedrohungsmodell, Komponentenübersicht, Datenmodell und Meilensteinplan
- Rust-Backend-Workspace-Skeleton (`backend/`) mit Health-Endpunkt
- Frontend-Skeleton (`frontend/`, Vite + React + TypeScript)
- CI-Grundgerüst (GitHub Actions: Build/Test für Backend und Frontend)
- Stub-Skripte für Installation/Update/Backup/Restore/Deinstallation (noch nicht produktionsreif)

### Bekannt / Noch ausstehend
- Kein funktionsfähiger Mailserver-Betrieb (Postfix/Dovecot/Rspamd sind noch nicht installiert/gestartet, nur konfiguriert)
- Kein REST-API-Funktionsumfang für Domain-/Benutzerverwaltung (folgt in M2)
- ACME/DKIM-Schlüsselerzeugung, SPF/DMARC/DNS-Assistent (folgt in M3)
- Installer führt noch keine echte Installation durch (folgt in M5)
