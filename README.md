# Havenmail

Havenmail ist eine eigenständig entwickelte, quelloffene Mailserver-Plattform für Debian 12/13. Sie orchestriert bewährte, aktiv gepflegte Mail-Engines (Postfix, Dovecot, Rspamd, ClamAV) über eine selbst entwickelte Control-Plane (Rust) mit REST-Admin-API und moderner Web-Oberfläche (React/TypeScript).

> **Projektstatus: früher Aufbau (M4 von M6).** Havenmail befindet sich in aktiver Entwicklung und ist **noch nicht produktionsreif**. Umgesetzt sind bisher das Repository-Grundgerüst, das Kern-Datenmodell (PostgreSQL-Migrationen), Auth-/RBAC-Bausteine, die Postfix-/Dovecot-/Rspamd-Konfigurationstemplates sowie eine REST-Admin-API samt CLI für Domain-/Benutzer-/Alias-/Weiterleitungsverwaltung (inkl. Mail-Loop-Schutz). Es gibt noch **keinen** Installer, keine OpenAPI-Spezifikation und keinen lauffähigen Mailserver-Betrieb (Postfix/Dovecot/Rspamd sind konfiguriert, aber nicht installiert) — siehe [CHANGELOG.md](CHANGELOG.md) und [docs/architecture.md](docs/architecture.md) für den genauen Stand je Meilenstein. Verwende Havenmail derzeit **nicht** für produktiven Mailbetrieb.

## Warum Havenmail

Havenmail schreibt keine eigene SMTP-, IMAP-, JMAP-, TLS- oder DKIM-Implementierung. Stattdessen konfiguriert und orchestriert es etablierte, battle-tested Engines:

- **Postfix** – SMTP, Submission (587), SMTPS (465)
- **Dovecot** – IMAP4rev1, JMAP, ManageSieve
- **Rspamd** – Spam-Filter, DKIM-Signierung/-Prüfung, SPF, DMARC
- **ClamAV** – Virenprüfung
- **acme.sh/certbot** – automatisierte TLS-Zertifikate

Eigenständig entwickelt sind: Datenmodell, Domain-/Benutzerverwaltung, REST-API, CLI, Backup/Restore, DNS-Assistent, Installer und die komplette Web-Oberfläche.

Details zu Architektur, Bedrohungsmodell, Datenmodell und Meilensteinen: siehe [docs/architecture.md](docs/architecture.md).

## Installation (Zielbild — noch nicht verfügbar)

```bash
curl -fsSL https://raw.githubusercontent.com/Fabio-Kumahost/havenmail/main/install.sh | sudo bash
```

Sicherere, empfohlene Variante — Skript vorher prüfen:

```bash
curl -fsSLo install.sh https://raw.githubusercontent.com/Fabio-Kumahost/havenmail/main/install.sh
less install.sh
sudo bash install.sh
```

`install.sh` ist vollständig implementiert; solange das Repo nicht auf GitHub veröffentlicht ist, funktioniert der obige Einzeiler noch nicht — siehe [docs/installation.md](docs/installation.md) für den heute funktionierenden Ablauf (Repo auf den Server kopieren, `sudo bash install.sh`) und den vollständigen Funktionsumfang.

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
