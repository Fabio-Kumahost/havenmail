# Changelog

Format angelehnt an [Keep a Changelog](https://keepachangelog.com/de/1.0.0/), Versionierung nach [SemVer](https://semver.org/lang/de/).

## [Unreleased]

### Hinzugefügt (M5: Installer & Betrieb)
- `install.sh` implementiert vollständig: Preflight, Self-Bootstrap (Single-File-curl vs. bestehendes Checkout), Systembenutzer/-verzeichnisse, apt-Pakete, Rust-/Node-Toolchain-Install, PostgreSQL-Rolle/DB, zweiphasiger nginx-/TLS-Rollout (Übergangs-vhost → certbot Webroot → voller HTTPS-Vhost), Backend-/Frontend-Build, Deployment der gerenderten Postfix-/Dovecot-/Rspamd-/Fail2ban-/nginx-Konfiguration an ihre Systempfade, Firewall (ufw), systemd-Unit, Dienststart, Health-Check
- `config/nginx/havenmail-http.conf.tera` (Übergangs-vhost für die ACME-Challenge) und `havenmail.conf.tera` (voller HTTPS-Vhost: reverse-proxied `/api/`, `/healthz`, `/readyz` zur Control-Plane, liefert den Frontend-Build mit SPA-Fallback aus). Frontend wird mit `VITE_HAVENMAIL_API_URL=""` gebaut — same-origin, kein CORS nötig
- Neuer CLI-Befehl `havenmail-cli render-configs`: rendert alle `config/*.tera`-Templates lokal (nutzt die bereits getestete `havenmail_core::config_render`-Logik) — keine zweite Template-Engine in Bash
- Neuer CLI-Befehl `havenmail-cli bootstrap-admin` + `havenmail_core::bootstrap`: legt idempotent die erste Domain und deren `super_admin`-Konto an; spricht direkt die DB an (kein unauthentifizierter API-Weg für die Kontoerstellung); generiertes Passwort landet ausschließlich in `/etc/havenmail/initial-admin-credentials` (0640), nie im Log
- `config/systemd/havenmail-api.service`: gehärtete systemd-Unit für die Control-Plane-API
- `backup.sh`/`restore.sh` implementiert: Archiv aus DB-Dump (`pg_dump --format=custom`) + `/etc/havenmail` (inkl. `HAVENMAIL_SECRETS_KEY`) + Maildaten, optionale gpg-Verschlüsselung, anzahlbasierte Retention; Restore verlangt explizite Bestätigung und verschiebt vorhandene Daten statt sie zu löschen
- `update.sh` implementiert: Versions-/Hauptversionsprüfung, automatisches Backup vor der Migration, Wartungsmodus (nur die API wird gestoppt — SMTP/IMAP laufen weiter), zweiphasiger Selbstneustart nach `git checkout` (vermeidet undefiniertes Verhalten durch Selbstmodifikation der laufenden Skriptdatei), automatischer Binary-Rollback bei Fehlern
- `uninstall.sh` implementiert: entfernt Havenmail-eigene Dienstteile/Konfigurationsfragmente; Nutzdaten (DB, Maildaten, Secrets) nur mit `--purge-data` und expliziter Bestätigung
- `shellcheck -x` über alle Installer-/Betriebsskripte: 0 Findings
- Neue API-Route `GET /api/v1/system/status` (nur `super_admin`, `Action::ManageSystemSettings`): Datenbankverbindung + `systemctl is-active` für alle orchestrierten Dienste (Postfix/Dovecot/Rspamd/ClamAV/nginx/Fail2ban/Control-Plane-API)
- Neue Admin-UI-Seite „System“: zeigt den Dienststatus aus der neuen Route
- Audit-Log an die API angebunden: `havenmail_core::audit::record` (neu) hängt Einträge transaktional + per Postgres-Advisory-Lock serialisiert an die Hash-Chain an (verhindert, dass gleichzeitige Requests die Kette verzweigen lassen). Migrationen `0003` (`seq`-Spalte für eindeutige Kettenreihenfolge) und `0004` (`domain_id`-Spalte für RBAC-Scoping). Verdrahtet in `domain.create/update/delete` und `user.create/update/delete` (Passwortänderungen landen nur als Aktionsname im Log, nie Klartext/Hash)
- Neue API-Route `GET /api/v1/audit-log` (`Action::ViewAuditLog`): `super_admin` sieht alles (optional nach `domain_id` gefiltert), `domain_admin` zwingend nur die eigene Domain
- Neue Admin-UI-Seite „Audit-Log“

### Bekannt / Noch ausstehend (nach M5)
- Kein End-to-End-Test des gesamten Installer-/Update-/Backup-Ablaufs auf einer frischen Debian-VM (nur einzelne Bausteine lokal gegen Dev-Postgres verifiziert, nginx-Konfiguration nur manuell geprüft — kein nginx auf der Entwicklungsmaschine verfügbar)
- Repo ist noch nicht auf GitHub veröffentlicht — der dokumentierte Single-File-curl-Einzeiler funktioniert erst danach (`HAVENMAIL_SOURCE_REPO`/`USERNAME`-Platzhalter)
- `update.sh --major`-Versionsvergleich ist rein heuristisch (String-Vergleich von `vX`-Tags), keine echte Signatur-/Release-Prüfung
- Backup: kein S3-Ziel, keine gestaffelte Retention, kein automatisierter Sandbox-Restore-Test, kein Einzel-Domain-Restore
- Audit-Log ist noch nicht an aliases/distribution_lists/forwards/dkim angebunden (nur domains/users) — kein Pagination-UI (Backend deckelt bei 200 Einträgen/Abfrage, UI zeigt aktuell fix die letzten 50)
- Admin-UI: Quotas-Übersicht, Warteschlangen, Zustellfehler, Spam-/Virenereignisse, TLS-Zertifikatslaufzeit, Backup-/Update-Status noch nicht umgesetzt

### Hinzugefügt (M4: Web-UI)
- React-Router-basierte Admin-Oberfläche: Login, Dashboard (API-Health), Domain-Liste/-Anlage, Domain-Detailseite mit Benutzer-/Alias-CRUD
- DNS-Einrichtungsassistent: kopierfertige Einträge (MX/SPF/DKIM/DMARC), DKIM-Schlüsselerzeugung per Klick, Live-DNS-Prüfung mit Ergebnisanzeige
- API-Client mit automatischem Access-Token-Refresh bei 401
- Light/Dark-Mode über `prefers-color-scheme`
- 3 Frontend-Tests (App-Routing, API-Fehlerbehandlung)

### Bekannt / Noch ausstehend (nach M4)
- Fehlende Admin-Bereiche aus der Aufgabenstellung: Quotas-Übersicht, Warteschlangen, Zustellfehler, Spam-/Virenereignisse, TLS-Zertifikate, System-/Audit-Protokolle, Backup-Status, Updates, Dienste/Ressourcenauslastung — benötigen teils noch fehlende Backend-Endpunkte bzw. installierte Mail-Engines (M5)
- Kein Verteiler-/Weiterleitungs-UI (nur über CLI/API verfügbar)
- Keine Suche/Filter/Sortierung/Pagination in den Tabellen
- Kein Wizard für Erst-Setup/Bootstrap des allerersten super_admin in der UI

### Hinzugefügt (M3: Sicherheit & Zustellbarkeit)
- DKIM-Schlüsselerzeugung (RSA-2048 über `rsa`-Crate), private Schlüssel ausschließlich AES-256-GCM-verschlüsselt gespeichert (`HAVENMAIL_SECRETS_KEY`)
- DNS-Prüfung (MX, SPF, DKIM, DMARC) über `hickory-resolver` mit Ergebnis-Historie in `dns_checks`
- DNS-Empfehlungsendpunkt liefert kopierfertige Einträge inkl. echtem DKIM-Wert nach Schlüsselerzeugung
- Neue API-Routen: `POST /domains/:id/dkim`, `GET /domains/:id/dns-recommendations`, `POST /domains/:id/dns-check`
- In-Process-Rate-Limiting für `/auth/login` (5 Versuche/15 Minuten je Client-IP, ermittelt über `X-Forwarded-For`)
- Fail2ban-Jail-Templates für Postfix-SASL und Dovecot (`config/fail2ban/`)
- 5 neue Unit-/Integrationstests (DKIM-Roundtrip, DNS-Empfehlungen, Rate-Limiter, Client-IP-Extraktion)

### Bekannt / Noch ausstehend (nach M3)
- MTA-STS/TLS-RPT-Policy-Hosting und ARC noch nicht umgesetzt
- DANE/DNSSEC-Unterstützung noch nicht umgesetzt
- Fail2ban-Templates sind Konfigurationsvorlagen; Installation/Aktivierung von fail2ban folgt mit dem Installer (M5)

### Hinzugefügt (M2: Domain-/Benutzerverwaltung + REST-API + CLI)
- REST-Admin-API (`/api/v1/...`): Login/Refresh/Logout (rotierende Refresh-Tokens), Domains (CRUD, super_admin-/domain_admin-Scope), Benutzer, Aliase, Verteiler, Weiterleitungen
- Weiterleitungen mit Loop-Schutz: verfolgt die Zielkette bis 25 Hops und lehnt Weiterleitungen ab, die eine Mail-Schleife erzeugen würden, sowie Weiterleitungen auf die eigene Adresse
- RBAC serverseitig durchgesetzt: domain_admin sieht/verwaltet nur die eigene Domain, kann keine super_admin-Konten anlegen; Fehlermeldungen für fremde Ressourcen wie "nicht gefunden" statt "keine Berechtigung" (Schutz vor Enumeration)
- CLI (`havenmail-cli`) spricht die REST-API an: `login`, `domain-create`, `domain-list`, `user-create`, `user-list`, `status`; Zugangstoken lokal unter `~/.config/havenmail/credentials.json` (0600)
- Integrationstests gegen echte PostgreSQL-Instanz: Login-Timing-Konsistenz, Domain-Scope-Isolation, Rechteausweitungsschutz, Loop-Schutz

### Bekannt / Noch ausstehend (nach M2)
- Kein Bootstrap-Mechanismus für den allerersten super_admin außer direktem DB-Zugriff (folgt mit dem Installer in M5)
- Keine OpenAPI-Spezifikation (geplant, noch nicht umgesetzt)
- CLI deckt bisher nur Domains/Benutzer ab, keine Aliase/Verteiler/Weiterleitungen

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
