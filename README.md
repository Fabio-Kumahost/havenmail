# Havenmail

Havenmail ist eine eigenständig entwickelte, quelloffene Mailserver-Plattform für Debian 12/13. Sie orchestriert bewährte, aktiv gepflegte Mail-Engines (Postfix, Dovecot, Rspamd, ClamAV) über eine selbst entwickelte Control-Plane (Rust) mit REST-Admin-API und moderner Web-Oberfläche (React/TypeScript).

> **Projektstatus: M5 von M6 abgeschlossen.** Installer, REST-Admin-API ([OpenAPI-Spezifikation](docs/openapi.yaml)), CLI, Web-Oberfläche (inkl. System-Status und Audit-Log), Backup/Restore/Update/Deinstallation sind implementiert und per Docker-Debian-12-Simulation sowie einer echten VM-Erstinstallation end-to-end verifiziert (siehe [CHANGELOG.md](CHANGELOG.md) für Details und dabei gefundene/behobene Bugs). Offen: M6-Abnahmetests (Open-Relay-Test, Zustellungstests) — siehe [docs/architecture.md](docs/architecture.md) für den genauen Stand je Meilenstein.

## Warum Havenmail

Havenmail schreibt keine eigene SMTP-, IMAP-, JMAP-, TLS- oder DKIM-Implementierung. Stattdessen konfiguriert und orchestriert es etablierte, battle-tested Engines:

- **Postfix** – SMTP, Submission (587), SMTPS (465)
- **Dovecot** – IMAP4rev1, JMAP, ManageSieve
- **Rspamd** – Spam-Filter, DKIM-Signierung/-Prüfung, SPF, DMARC
- **ClamAV** – Virenprüfung
- **acme.sh/certbot** – automatisierte TLS-Zertifikate

Eigenständig entwickelt sind: Datenmodell, Domain-/Benutzerverwaltung, REST-API, CLI, Backup/Restore, DNS-Assistent, Installer und die komplette Web-Oberfläche.

Details zu Architektur, Bedrohungsmodell, Datenmodell und Meilensteinen: siehe [docs/architecture.md](docs/architecture.md).

## Installation

```bash
curl -fsSL https://raw.githubusercontent.com/Fabio-Kumahost/havenmail/main/install.sh | sudo bash
```

Sicherere, empfohlene Variante — Skript vorher prüfen:

```bash
curl -fsSLo install.sh https://raw.githubusercontent.com/Fabio-Kumahost/havenmail/main/install.sh
less install.sh
sudo bash install.sh
```

Details, Voraussetzungen (DNS, offene Ports) und der vollständige Ablauf: [docs/installation.md](docs/installation.md).

## Repository-Struktur

```
backend/    Rust Control-Plane (API, Datenmodell, Orchestrierung, CLI)
frontend/   React/TypeScript Admin-Oberfläche
config/     Konfigurationstemplates für Postfix/Dovecot/Rspamd/nginx
scripts/    Provisioning- und Preflight-Hilfsskripte
docs/       Architektur-, Betriebs- und Sicherheitsdokumentation
```

## Lizenz

AGPL-3.0-or-later, siehe [LICENSE](LICENSE).

## Sicherheit

Verantwortungsvolle Offenlegung von Schwachstellen: siehe [SECURITY.md](SECURITY.md).
