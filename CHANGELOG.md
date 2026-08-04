# Changelog

Format angelehnt an [Keep a Changelog](https://keepachangelog.com/de/1.0.0/), Versionierung nach [SemVer](https://semver.org/lang/de/).

## [Unreleased]

### Hinzugefügt
- Repository-Grundgerüst: README, LICENSE (AGPL-3.0-or-later), SECURITY.md, CONTRIBUTING.md
- Architekturdokumentation (`docs/architecture.md`) mit Bedrohungsmodell, Komponentenübersicht, Datenmodell und Meilensteinplan
- Rust-Backend-Workspace-Skeleton (`backend/`) mit Health-Endpunkt
- Frontend-Skeleton (`frontend/`, Vite + React + TypeScript)
- CI-Grundgerüst (GitHub Actions: Build/Test für Backend und Frontend)
- Stub-Skripte für Installation/Update/Backup/Restore/Deinstallation (noch nicht produktionsreif)

### Bekannt / Noch ausstehend
- Kein funktionsfähiger Mailserver-Betrieb (Postfix/Dovecot/Rspamd-Orchestrierung folgt in M1)
- Kein Datenmodell/Migrationen (folgt in M1)
- Kein REST-API-Funktionsumfang (folgt in M2)
- Installer führt noch keine echte Installation durch (folgt in M5)
