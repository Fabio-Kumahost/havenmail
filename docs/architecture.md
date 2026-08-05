# Havenmail — Architektur

## Überblick

Havenmail ist eine produktionsreif geplante, quelloffene Mailserver-Plattform mit moderner Weboberfläche, die sich funktional an etablierten Lösungen (z. B. Stalwart) orientiert, aber vollständig eigenständig entwickelt wird (Architektur, Code, Texte, Branding, UI). Kernanforderung: **keine ungeprüfte Eigenimplementierung protokollkritischer/kryptografischer Funktionen** (SMTP, IMAP, JMAP, TLS, DKIM, Auth). Der Server soll per Ein-Zeilen-Installer auf einem frischen Debian-12/13-VPS lauffähig sein, mehrere Domains/Benutzer verwalten und Administration sowie optional Webmail über eine hochwertige Oberfläche anbieten.

Grundsatzentscheidungen:
- **Engine-Strategie:** Orchestrierte, battle-tested Standard-Daemons (Postfix, Dovecot, Rspamd, ACME-Client) hinter einer vollständig eigenständigen Control-Plane (API, Datenmodell, UI, Installer). Kein selbstgeschriebener Protokollcode.
- **Webmail:** Nicht im MVP; architektonisch vorbereitet (JMAP steht bereit), als eigenes Modul für eine Folgephase geplant.
- **Stack:** Rust-Backend (axum, sqlx) für die Control-Plane, TypeScript/React-Frontend für die Web-UI.

Dieses Dokument ist die verbindliche Referenz für Architektur, Komponenten, Sicherheitsmodell, Datenmodell, Repo-Struktur, MVP-Abgrenzung, Meilensteine und Risiken. Der Umsetzungsstatus je Meilenstein wird in [CHANGELOG.md](../CHANGELOG.md) nachgeführt.

## Anforderungen, Annahmen, Nicht-Ziele

**Kernanforderungen:** siehe Originalauftrag (SMTP/Submission/SMTPS, IMAP4rev1, JMAP, Autodiscover/Autoconfig, Domain-/Benutzerverwaltung mit RBAC, TLS/DKIM/SPF/DMARC/MTA-STS/TLS-RPT/DANE, Anti-Spam/-Virus, Rate-Limiting/Brute-Force-Schutz, moderne responsive Admin-UI, REST-API mit OpenAPI, CLI, Backup/Restore, Updates mit Rollback, Debian-Installer).

**Annahmen (dokumentiert, da keine architekturkritische Rückfrage nötig):**
- Zielplattform: Debian 12 (bookworm) und 13 (trixie), amd64 und arm64, native systemd-Installation als Standard (siehe Begründung unten); Docker Compose als dokumentierte Alternative für Testing/Staging.
- Datenbank: PostgreSQL 16 (stabil, verbreitet, gute Backup-Werkzeuge) als primärer Metadatenspeicher; Maildir-Format auf Disk für Mailspeicherung (Dovecot-Standard, kompatibel mit klassischen Backup-Tools).
- Lizenz: AGPL-3.0-or-later für Server-Code (verhindert unfreie SaaS-Weiterverwertung ohne Codefreigabe, üblich bei vergleichbaren Self-Hosting-Mailprojekten), MIT für CLI/SDK-Bibliotheken. Kann vom Nutzer jederzeit geändert werden.
- Repository: [github.com/Fabio-Kumahost/havenmail](https://github.com/Fabio-Kumahost/havenmail) (ursprünglich als `USERNAME/REPOSITORY`-Platzhalter dokumentiert, seit M5 aufgelöst).
- Single-Node-Deployment im MVP (kein Multi-Node-Clustering); Skalierungsgrenzen werden dokumentiert, nicht gelöst.
- ClamAV für Virenprüfung (aktiv gepflegt, Standard in diesem Bereich); Rspamd übernimmt Spam/DKIM/SPF/DMARC/ARC-Auswertung und -Signierung.

**Nicht-Ziele (MVP):**
- Kein Webmail-Client (folgt als separates Modul).
- Kein Multi-Node-/HA-Clustering.
- Keine eigene Krypto-, TLS-, SMTP-, IMAP- oder JMAP-Engine.
- Kein Windows/macOS-Server-Support.

## Bedrohungsanalyse (Kurzfassung)

| Bedrohung | Gegenmaßnahme |
|---|---|
| Open Relay / Spam-Abuse | Postfix `smtpd_relay_restrictions` standardmäßig restriktiv, SASL-Auth-Pflicht für Submission, Rspamd-Ratenlimits |
| Zugangsdaten-Diebstahl / Brute-Force | Argon2id-Passwort-Hashes, Fail2ban-Filter für Postfix/Dovecot/Admin-API, konstante-Zeit-Vergleiche, Account-Lockout mit Backoff |
| Mail-Spoofing / Phishing | SPF/DKIM/DMARC/ARC via Rspamd, MTA-STS + TLS-RPT, DANE optional bei DNSSEC |
| Kompromittierte Admin-UI (XSS/CSRF/Clickjacking) | CSP, SameSite-Cookies, CSRF-Token, X-Frame-Options/Frame-Ancestors, Input-Validierung serverseitig |
| SSRF über Admin-Funktionen (z. B. DNS-Check, Webhooks) | Egress-Allowlist, kein Follow von Redirects auf private IP-Ranges, Timeouts |
| Kompromittierte API-Tokens | Kurzlebige JWTs + Refresh, API-Keys mit Scopes und Widerruf, Audit-Log |
| Mail-Loops bei Weiterleitungen | Loop-Erkennung über Hop-Zähler/History vor Aktivierung einer Weiterleitung |
| Benutzer-Enumeration | Einheitliche Fehlermeldungen bei Login/Passwort-Reset, konstante Antwortzeiten |
| Kompromittiertes Installer-Skript (Supply-Chain) | Signierte Releases, SHA-256-Checksummen, dokumentierte sichere Installationsvariante, Pin auf Release-Tag statt `main` |
| Datenverlust | Verschlüsselte, automatisierte Backups mit Restore-Test |

## Architekturentscheidung

**Orchestrierte Control-Plane über bewährte Mail-Daemons** (kein Eigenbau von Protokollcode):

```
                        ┌─────────────────────────┐
Internet ── 25/587/465/143/993/4190 ──▶ Postfix + Dovecot + Rspamd + ClamAV
                        │  (systemd-Dienste, von Control-Plane konfiguriert) │
                        └─────────────┬────────────┘
                                      │ config render (Lua/Maps/SQL-Views), Milter, Dovecot-Auth (checkpassword/SQL)
                        ┌─────────────▼────────────┐
                        │  Control-Plane (Rust)     │  ← eigenständig entwickelt
                        │  - REST/JMAP-Admin-API    │
                        │  - Domain/User/Alias-CRUD │
                        │  - Config-Renderer        │
                        │  - ACME-Orchestrierung    │
                        │  - DNS-Checker            │
                        │  - Backup/Restore-Engine  │
                        │  - Auth (Argon2id, TOTP)  │
                        └─────────────┬────────────┘
                                      │ PostgreSQL (Domains, User, Aliase, Audit, Tokens)
                        ┌─────────────▼────────────┐
                        │  Web-UI (React/TS SPA)    │  ← eigenständig entwickelt, eigenes Branding
                        └───────────────────────────┘
```

Die Control-Plane **schreibt** Postfix/Dovecot/Rspamd-Konfiguration (Maps, TLS-Zertifikatspfade, SQL-Views für virtuelle Domains/User) und **liest** deren Status (Queues, Logs via strukturiertem Log-Parsing/Postfix-`mailq`, Dovecot-`doveadm`) — es gibt keinen selbstgeschriebenen SMTP/IMAP/JMAP-Stack. JMAP wird über Dovecots eingebautes JMAP-Modul bereitgestellt (Dovecot ≥ 2.3 mit Pro/CE-JMAP-Plugin bzw. Dovecot-Community-JMAP, in Implementierungsphase konkret zu verifizieren; Fallback dokumentiert, s. Risiken).

**Warum native systemd-Installation als Standard (statt Container):** Mailserver benötigen privilegierte Ports (25/465/587) und enge Kernel-/Netzwerkinteraktion (rDNS, echte Client-IP für RBLs), was in Containern zusätzliche Netzwerk-Komplexität (macvlan/host-Netzwerk) erzeugt und die Angriffsfläche eher vergrößert als verkleinert; native systemd-Dienste mit hardening (`ProtectSystem`, `NoNewPrivileges`, dedizierte Unix-User je Dienst) sind für dieses Szenario etablierter und wartungsfreundlicher. Docker Compose wird trotzdem vollständig dokumentiert und unterstützt (Postfix/Dovecot/Rspamd/ClamAV/Postgres/Control-Plane als Container mit `network_mode: host` für die Mail-Ports oder expliziten Portmappings), ohne `latest`-Tags, unprivilegiert wo möglich.

### Komponenten & Lizenzen

| Komponente | Rolle | Lizenz |
|---|---|---|
| Postfix | SMTP (eingehend, Submission, SMTPS) | IBM Public License / EPL |
| Dovecot CE | IMAP4rev1, JMAP, ManageSieve | LGPL-2.1 / MIT (Teile) |
| Rspamd | Spam-Filter, DKIM-Sign/-Verify, SPF, DMARC, ARC, Greylisting | Apache-2.0 |
| ClamAV | Virenscan (Milter-Anbindung) | GPL-2.0 |
| acme.sh oder certbot | ACME/Let's-Encrypt-TLS | GPL-3.0 / Apache-2.0 |
| PostgreSQL | Metadaten, Audit-Log, Tokens | PostgreSQL License |
| fail2ban | Brute-Force-Schutz | GPL-2.0 |
| Eigene Control-Plane (Rust, axum, sqlx) | Orchestrierung, API, Auth, Backup | AGPL-3.0 (Vorschlag) |
| Eigenes Web-UI (React/TS) | Administration | AGPL-3.0 (Vorschlag) |
| Eigene CLI (Rust, clap) | Administration ohne UI | AGPL-3.0 (Vorschlag) |

Alle Komponenten aktiv gepflegt, stabile Debian-12/13-Pakete oder offiziell signierte Upstream-Repos, Versionen werden im Installer/Ansible-artigen Provisioning-Skript gepinnt.

### Sicherheitsmodell

- **AuthN Admin-UI/API:** Argon2id-Passwort-Hashes, optionale TOTP-2FA, kurzlebige JWT-Access-Tokens (15 min) + Refresh-Tokens (rotierend, an Session gebunden, widerrufbar), App-Passwörter (separater Scope, nur für Mail-Clients) mit eigenem Hash.
- **AuthZ:** RBAC mit Rollen `super_admin`, `domain_admin`, `user`; jede API-Operation prüft Domain-Scope serverseitig (kein Vertrauen auf Client-seitige Filterung).
- **Mail-AuthN:** Dovecot SASL gegen PostgreSQL-View (Argon2id via Dovecots `ARGON2ID`-Scheme), Postfix nutzt Dovecot-SASL für Submission.
- **Transport:** TLS 1.2+ (Default TLS 1.3) über zentral verwaltete Zertifikate (ACME), STARTTLS erzwungen auf 587/143, implizites TLS auf 465/993.
- **Web-Security-Header:** CSP (strict, nonce-basiert), `X-Content-Type-Options`, `Referrer-Policy`, `Permissions-Policy`, `SameSite=Strict`+`Secure`+`HttpOnly`-Cookies, CSRF-Token für state-changing Requests.
- **Audit:** Jede administrative Änderung (Domain/User/Alias/Config/Token) wird unveränderlich (append-only, mit Hash-Chain) protokolliert.
- **Secrets:** Keine Klartext-Zugangsdaten in Logs/Mails; initiale Admin-Zugangsdaten werden beim Setup einmalig angezeigt/in geschützte Datei geschrieben, nie per Mail versendet.

### Portübersicht

| Port | Protokoll | Zweck | Standard |
|---|---|---|---|
| 25/tcp | SMTP | Eingehende Mail (MX) | offen, kein Relay ohne Auth |
| 587/tcp | Submission | Mailversand mit STARTTLS+SASL | offen |
| 465/tcp | SMTPS | Mailversand implizites TLS | offen (optional deaktivierbar) |
| 143/tcp | IMAP+STARTTLS | Mailabruf | offen |
| 993/tcp | IMAPS | Mailabruf implizites TLS | offen |
| 4190/tcp | ManageSieve | Serverseitige Filter | optional, standardmäßig nur localhost/VPN |
| 443/tcp | HTTPS | Admin-UI, REST/JMAP-API, Autoconfig/Autodiscover, ACME-HTTP-01 | offen |
| 8080/tcp (intern) | Control-Plane-API | nur localhost, hinter 443-Reverse-Proxy (nginx/caddy) | intern |
| 5432/tcp | PostgreSQL | nur localhost/Unix-Socket | intern |
| 11332-11334/tcp | Rspamd | nur localhost (Milter/Worker) | intern |

### Datenmodell (Kernentitäten)

- `domains` (id, name, is_active, catch_all_enabled, dkim_selector, quota_bytes, created_at)
- `users` (id, domain_id, local_part, password_hash, role, quota_bytes, is_active, totp_secret_enc, created_at)
- `aliases` (id, domain_id, source, destination[], is_active)
- `distribution_lists` (id, domain_id, address, members[])
- `forwards` (id, user_id, target_address, keep_copy, loop_guard_hash)
- `api_tokens` (id, user_id, scopes[], hash, expires_at, revoked_at)
- `sessions` (id, user_id, refresh_token_hash, ip, user_agent, created_at, revoked_at)
- `dkim_keys` (id, domain_id, selector, private_key_enc, public_key, active)
- `audit_log` (id, actor_id, action, target, before, after, ip, created_at, prev_hash, hash)
- `backup_runs` (id, started_at, finished_at, status, size_bytes, target)
- `dns_checks` (id, domain_id, record_type, expected, actual, status, checked_at)

Verschlüsselung sensibler Spalten (private DKIM-Keys, TOTP-Secrets) via AEAD (age/libsodium-Wrapper) mit Master-Key aus Systemd-Credentials oder KMS-kompatiblem Secret-Store.

## Repository-Struktur

```
/
├── README.md, LICENSE, SECURITY.md, CONTRIBUTING.md, CHANGELOG.md
├── install.sh, uninstall.sh, update.sh, backup.sh, restore.sh
├── docker-compose.yml, .env.example
├── docs/ (architecture, installation, dns-setup, security, backup-restore, upgrades, troubleshooting)
├── scripts/          # Provisioning-Helfer, Debian-Preflight-Checks
├── config/           # Templates für Postfix/Dovecot/Rspamd/nginx (von Control-Plane gerendert)
├── backend/           # Rust Control-Plane (axum, sqlx, CLI-Crate, Migrations)
├── frontend/           # React/TS Admin-UI
└── .github/workflows, ISSUE_TEMPLATE, dependabot.yml
```

## MVP-Abgrenzung

**Im MVP:** Domain-/User-/Alias-/Verteiler-Verwaltung, SMTP/Submission/SMTPS, IMAP+ManageSieve, JMAP für Mail/Identities (kein Webmail-Frontend), DKIM/SPF/DMARC/MTA-STS/TLS-RPT-Assistent inkl. DNS-Check, ACME-TLS, RBAC (super_admin/domain_admin/user), TOTP-2FA, App-Passwörter, Audit-Log, Rate-Limiting/Fail2ban, REST-Admin-API mit OpenAPI + CLI, Health/Metrics-Endpunkte, Backup/Restore (lokal + S3-kompatibel), Debian-12/13-Installer (systemd), Update/Rollback-Grundgerüst, Docker-Compose-Alternative.

**Explizit nach MVP:** Webmail-Modul, DANE, ARC, Multi-Node/HA, Prometheus-Dashboards (Grafana-Vorlagen), Webhooks.

## Umsetzungsplan mit Meilensteinen

1. **M0 – Grundgerüst:** Repo-Skeleton, Lizenzen, CI-Grundgerüst (lint/build/test), Rust-Workspace + React-App-Skeleton, Postgres-Migrations-Tooling.
2. **M1 – Sicherer Kern:** Postfix/Dovecot/Rspamd/ClamAV-Konfigurationstemplates, SQL-Views für virtuelle Domains/User, Auth (Argon2id, JWT, TOTP), RBAC, Audit-Log, Migrations für Kern-Datenmodell.
3. **M2 – Domain-/Benutzerverwaltung + API:** REST-Admin-API + OpenAPI-Doku, CLI, Alias/Verteiler/Forward mit Loop-Schutz, Quotas.
4. **M3 – Sicherheit & Zustellbarkeit:** ACME-Orchestrierung, DKIM-Key-Management, SPF/DMARC/MTA-STS/TLS-RPT-Assistent, DNS-Checker, Fail2ban-Integration, Rate-Limiting.
5. **M4 – Web-UI:** Alle in der Aufgabenstellung geforderten Admin-Bereiche (Dashboard, Domains, User, Aliase, Quotas, Queues, Zustellfehler, Spam/Virus-Events, DKIM/SPF/DMARC-Status, DNS-Wizard, TLS, Logs/Audit, Backup-Status, Updates, Dienste/Ressourcen), Light/Dark-Mode, A11y.
6. **M5 – Installer & Betrieb:** `install.sh` (Preflight, unattended-Modus, idempotent), `update.sh`, `uninstall.sh`, `backup.sh`/`restore.sh`, Docker-Compose-Variante, signierte Releases + SHA-256 + SBOM.
7. **M6 – Tests & Abnahme:** Unit/Integration/E2E, Protokolltests (SMTP/IMAP/JMAP), Open-Relay-Tests, Mandantentrennungstests, Install/Update/Backup/Restore-Tests, frische Debian-VM-Abnahme gemäß Abnahmekriterien.
8. **M7 – Webmail-Modul (Folgephase, außerhalb MVP):** separates Frontend über JMAP.

Jeder Meilenstein schließt mit grünem Build + Tests ab, bevor der nächste beginnt.

## Bekannte Risiken

- **JMAP-Reife in Dovecot CE:** muss in M1 früh technisch verifiziert werden (Lizenzmodell, Funktionsumfang); Fallback: JMAP-Proxy-Layer, die JMAP-Requests in IMAP/ManageSieve-Operationen übersetzt (weiterhin ohne eigene Krypto/Auth-Neuimplementierung).
- **Aufwand:** Der volle Funktionsumfang ist sehr groß; MVP-Scoping (s. o.) ist notwendig, um in überschaubaren Schritten lauffähige, getestete Zwischenstände zu liefern.
- **Rechtliches/Lizenz:** AGPL-Vorschlag sollte vom Projektinhaber bestätigt werden, bevor Fremdcode/Contributions angenommen werden.
- **Frische-VM-Abnahme:** setzt eine reale Debian-Testumgebung voraus, die in dieser Sitzung nicht automatisch bereitsteht — wird als expliziter, dokumentierter Prüfschritt in M6 behandelt.

## Verifikation

- Nach jedem Meilenstein: `cargo build && cargo test` (Backend), `npm run build && npm test` (Frontend), CI-Workflows grün.
- M1: manueller SMTP/IMAP-Handshake-Test gegen lokale VM/Container (`swaks`, `openssl s_client`, `doveadm`).
- M3: DNS-Checker gegen Testdomain mit realen DNS-Einträgen verifizieren.
- M6: vollständige Abnahme auf frischer Debian-12/13-VM gemäß den in der Aufgabenstellung genannten Abnahmekriterien (Open-Relay-Test, TLS-Test, DKIM-Test, Backup/Restore-Test, Rechtetrennungstest). Automatisiert per `scripts/acceptance-test.sh` (läuft auf dem installierten Server, deckt Open-Relay/TLS/DKIM/Rechtetrennung ab; Backup/Restore bewusst separat über `backup.sh`/`restore.sh`, da ein Test hier reale Daten überschreiben würde).
