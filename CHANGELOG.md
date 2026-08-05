# Changelog

Format angelehnt an [Keep a Changelog](https://keepachangelog.com/de/1.0.0/), Versionierung nach [SemVer](https://semver.org/lang/de/).

## [Unreleased]

### Behoben — durch echten Installer-Testlauf in einer Debian-12-Container-Umgebung mit systemd gefunden
`install.sh` wurde erstmals nicht nur gegen Syntax/Einzelbausteine, sondern als vollständiger Ablauf in einer möglichst realitätsnahen Debian-12-Umgebung (systemd als PID 1, echte apt-Pakete, echter Build) durchgespielt — dabei kamen sechs reale Bugs zum Vorschein, die auf einem frischen Server genauso aufgetreten wären:
- `openssl` ist auf einem Minimal-Debian nicht garantiert vorinstalliert, wurde aber schon vor der Paketinstallation für die Secret-Generierung gebraucht → `install.sh` installiert Pakete jetzt vor dem Schreiben der Env-Datei; `openssl` zusätzlich explizit im Paketbedarf
- `sudo` ist auf einem Minimal-Debian ebenfalls nicht vorinstalliert → alle `sudo -u postgres`-Aufrufe (`install_steps.sh`, `uninstall.sh`) durch `runuser -u postgres --` ersetzt (util-linux, immer vorhanden; das Skript läuft ohnehin bereits als root)
- `RUSTUP_HOME`/`CARGO_HOME` waren nur lokal in `havenmail_install_rust_toolchain` gesetzt — in einer neuen Shell (z. B. `update.sh`) fand der `cargo`-Symlink seine Toolchain nicht mehr ("no default is configured") → jetzt unbedingt in `common.sh` exportiert
- Debians `nodejs`-apt-Paket (Bookworm: v18) ist zu alt für den Vite/Rolldown-Frontend-Build (braucht Node ≥ 20, real fehlgeschlagen mit `node:util does not provide an export named 'styleText'`) und zieht nebenbei hunderte transitive `node-*`-Pakete → `havenmail_install_node` nutzt jetzt NodeSource (Node 22.x LTS)
- `clamav-daemon` startet nicht, solange keine Signaturdatenbank existiert (`ConditionPathExistsGlob` auf `/var/lib/clamav/daily.{cvd,cld}`) → neue Funktion `havenmail_provision_clamav` lädt die Datenbank einmalig per `freshclam`, bevor der Dienst gestartet wird, und aktiviert `clamav-freshclam.service` für künftige Updates
- `fail2ban` wurde installiert, aber nie gestartet/aktiviert → jetzt explizit in `havenmail_start_services` enthalten

Nach diesen Fixes lief der komplette Installer-Ablauf (Pakete → Build → Config-Rendering → nginx-Bootstrap → TLS → Mail-Configs → systemd → Health-Check → Admin-Bootstrap) erfolgreich durch, inklusive erfolgreichem Login über die echte, per Installer aufgesetzte API und korrekt befülltem System-Status-Endpunkt (alle Dienste `active`, TLS-Ablaufdatum korrekt berechnet). TLS-Ausstellung war dabei simuliert (selbstsigniertes Zertifikat statt echtem Let's-Encrypt-Lauf, da der Testcontainer keine öffentliche IP/kein Reverse-DNS hat) — das ist weiterhin ungetestet und der nächste Schritt auf einer echten VM. `fail2ban` lief in einem zweiten Testcontainer mit installiertem `openssh-server`/`rsyslog` (näher an einer echten VM) sauber an — der vorherige Fehlschlag war ein Artefakt des minimalen Containers ohne Log-Quelle für die Default-`sshd`-Jail.

### Behoben — kritisch, durch echten Backup/Restore-Testlauf gefunden
- **`restore.sh` setzte die Berechtigungen von `/etc` (system­weit!) und `/var/mail` auf `0750 root:havenmail` bzw. `havenmail:havenmail` zurück.** Ursache: `install -d -m … "$(dirname "$HAVENMAIL_ETC_DIR")"` — `dirname` von `/etc/havenmail` ist `/etc`, und `install -d` chmod/chown't ein bereits existierendes Zielverzeichnis auf die angegebenen Werte, statt es nur bei Bedarf anzulegen. Real aufgetreten: nach einem Test-Restore verlor u. a. `psql` (Perl-Wrapper unter Debian) den Lesezugriff auf `/etc/perl` und schlug fehl. Fix: diese beiden `install -d`-Aufrufe ersatzlos entfernt (die Elternverzeichnisse existieren auf jedem laufenden System bereits; `cp -a` legt das eigentliche Zielverzeichnis mit den richtigen Rechten selbst an). Verifiziert: `/etc`/`/var/mail` bleiben jetzt unangetastet, Restore stellt die Domain korrekt wieder her, alle Dienste laufen danach weiter
- `install.sh`/`update.sh`/`restore.sh`: `--help` dokumentierte für `--version`/`--target` die Leerzeichen-Syntax (`--target <datei>`), der Parser akzeptierte aber nur `--target=datei` — beim eigenen End-to-End-Test von `restore.sh` selbst aufgefallen. Alle drei Skripte akzeptieren jetzt beide Schreibweisen (`install.sh` sichert dafür `$@` vor dem Parsen in `ORIGINAL_ARGS`, da es für den Self-Bootstrap-Re-Exec weiter unten noch gebraucht wird)
- macOS-Metadaten-Dateien (`._*`, AppleDouble-Sidecars von `tar`/Finder) im `migrations`-Verzeichnis brechen `sqlx::migrate!` ("expected integer version prefix") — trat sowohl im Testcontainer (Kopie per `tar`) als auch beim Nutzer auf der echten VM (Transfer per `tar über ssh` vom Mac) auf. Kein Codefix nötig (kein Repo-Inhalt, git verfolgt diese Dateien nicht), aber docs/installation.md ergänzt: `COPYFILE_DISABLE=1` beim `tar`-Transfer vom Mac setzen

### Audit-Log jetzt vollständig (alle Mutationen)
`alias.create/delete`, `distribution_list.create/delete`, `forward.create/delete`, `dkim.generate` protokolliert (bisher nur `domain.*`/`user.*`). Neuer Integrationstest deckt alle vier neuen Ressourcentypen ab.

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
- TLS-Zertifikatslaufzeit im System-Status: certbot-Deploy-Hook (`havenmail_install_tls_expiry_hook`) schreibt bei jeder Ausstellung/Erneuerung nur das Ablaufdatum nach `/etc/havenmail/tls-expiry` (0644) — die API braucht dafür keinen Lesezugriff auf `/etc/letsencrypt` (bleibt root:root 0700, enthält den privaten Schlüssel)

### Bekannt / Noch ausstehend (nach M5)
- Vollständiger Installer-Durchlauf in einer Debian-12-Container-Simulation (systemd, echte Pakete) erfolgreich — echte VM mit öffentlicher IP/DNS für echtes Let's-Encrypt sowie `update.sh`/`backup.sh`/`restore.sh`/`uninstall.sh` end-to-end noch nicht getestet
- `fail2ban` schlug im Testcontainer beim Start fehl ("Have not found any log file for sshd jail" — die Default-`sshd`-Jail erwartet `/var/log/auth.log`, das im minimalen Container ohne rsyslog/SSH-Server fehlt). Vermutlich Container-Artefakt, auf einer echten VM mit laufendem SSH-Dienst noch zu verifizieren
- Repo ist noch nicht auf GitHub veröffentlicht — der dokumentierte Single-File-curl-Einzeiler funktioniert erst danach (`HAVENMAIL_SOURCE_REPO`/`USERNAME`-Platzhalter)
- `update.sh --major`-Versionsvergleich ist rein heuristisch (String-Vergleich von `vX`-Tags), keine echte Signatur-/Release-Prüfung
- Backup: kein S3-Ziel, keine gestaffelte Retention, kein automatisierter Sandbox-Restore-Test, kein Einzel-Domain-Restore
- Audit-Log: kein Pagination-UI (Backend deckelt bei 200 Einträgen/Abfrage, UI zeigt aktuell fix die letzten 50)
- Admin-UI: Quotas-Übersicht, Warteschlangen, Zustellfehler, Spam-/Virenereignisse, Backup-/Update-Status noch nicht umgesetzt

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
