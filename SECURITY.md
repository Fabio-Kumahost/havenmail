# Security Policy

## Meldung von Sicherheitslücken

Bitte melde Sicherheitslücken **nicht** über öffentliche GitHub-Issues.

Nutze stattdessen den privaten Meldeweg über GitHub Security Advisories:
`Security` → `Report a vulnerability` in diesem Repository.

Bitte gib nach Möglichkeit an:
- Betroffene Komponente (Control-Plane, Web-UI, Installer, Konfigurationstemplates)
- Havenmail-Version bzw. Commit-Hash
- Reproduktionsschritte oder Proof-of-Concept
- Erwartete vs. tatsächliche Auswirkung

## Reaktionszeiten (Ziel)

- Eingangsbestätigung: innerhalb von 3 Werktagen
- Erste Einschätzung (Schweregrad, betroffene Versionen): innerhalb von 10 Werktagen
- Koordinierte Offenlegung nach Fix-Verfügbarkeit

## Umfang

Als Sicherheitslücken in Havenmail selbst gelten Schwachstellen in:
- der Rust-Control-Plane (`backend/`)
- der Web-Oberfläche (`frontend/`)
- den Installations-/Update-/Backup-Skripten
- den mitgelieferten Konfigurationstemplates (`config/`), sofern sie zu unsicheren Defaults führen

Schwachstellen in orchestrierten Drittkomponenten (Postfix, Dovecot, Rspamd, ClamAV, PostgreSQL) bitte direkt an die jeweiligen Upstream-Projekte melden; Hinweise auf unsichere *Konfiguration* dieser Komponenten durch Havenmail sind hier trotzdem willkommen.

## Unterstützte Versionen

Solange Havenmail Pre-1.0 ist, wird ausschließlich der `main`-Branch mit Sicherheitsfixes versorgt. Nach dem ersten stabilen Release wird diese Tabelle aktualisiert.
