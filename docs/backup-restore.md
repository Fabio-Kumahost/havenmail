# Backup & Restore

> **Status (M5):** `backup.sh`/`restore.sh` sind implementiert und decken den Kernfall ab (lokales, optional verschlüsseltes Archiv aus DB-Dump + `/etc/havenmail` + Maildaten). S3-Ziel, gestaffelte Retention und automatisierte Sandbox-Restore-Tests aus dem ursprünglichen Zielbild sind noch nicht umgesetzt.

## Was gesichert wird

Ein Lauf von `./backup.sh` erzeugt ein einziges Archiv `havenmail-<UTC-Zeitstempel>.tar.gz[.gpg]` unter `HAVENMAIL_BACKUP_DIR` (Standard `/var/backups/havenmail`) mit:

- `db.dump` — `pg_dump --format=custom` der gesamten Datenbank (Domains, Benutzer, Aliase, Verteiler, Weiterleitungen, **verschlüsselte DKIM-Privatschlüssel**, Audit-Log, …)
- `etc/havenmail/` — Env-Datei (inkl. `HAVENMAIL_SECRETS_KEY`, ohne den die DKIM-Schlüssel aus der DB nicht entschlüsselbar sind) und die Erstzugangsdaten-Datei
- `var/mail/havenmail/` — Maildir-Daten

```bash
./backup.sh              # Backup erstellen
./backup.sh --list        # Vorhandene Backups auflisten
```

### Verschlüsselung

Ist `HAVENMAIL_BACKUP_PASSPHRASE` gesetzt, wird das Archiv per gpg (AES-256, symmetrisch) verschlüsselt. Ohne diese Variable liegt es **unverschlüsselt** vor — `backup.sh` gibt dafür eine deutliche Warnung aus. Für Produktivbetrieb:

```bash
export HAVENMAIL_BACKUP_PASSPHRASE="$(openssl rand -base64 32)"
# sicher verwahren (Passwort-Manager/Secrets-Store), NICHT im Repo oder in .env
```

### Retention

Anzahl-basiert: `HAVENMAIL_BACKUP_RETENTION` (Standard 14) — ältere Archive werden nach jedem erfolgreichen Lauf automatisch gelöscht. Eine Tages-/Wochen-/Monats-Staffelung wie ursprünglich geplant ist noch nicht umgesetzt.

### Automatisierung (Cron)

```cron
0 3 * * * root HAVENMAIL_BACKUP_PASSPHRASE="…" /opt/havenmail/backup.sh >> /var/log/havenmail/backup.log 2>&1
```

## Restore

```bash
./restore.sh --list
./restore.sh --target havenmail-20260805T030000Z.tar.gz.gpg
```

Verlangt eine interaktive Bestätigung (`JA` eingeben), außer mit `--force`. Vorhandene Daten werden **nicht gelöscht**, sondern vor dem Überschreiben nach `*.before-restore-<Zeitstempel>` verschoben (`/etc/havenmail`, `/var/mail/havenmail`) — nach Prüfung manuell entfernen. Die Datenbank wird per `pg_restore --clean --if-exists` in-place ersetzt.

### Disaster-Recovery-Prozess

1. Neuestes Backup identifizieren: `./restore.sh --list`
2. `./restore.sh --target <backup-datei>`, Bestätigung mit `JA`
3. Skript stoppt Dienste, stellt DB/Config/Maildaten wieder her, startet Dienste neu
4. Health-Check prüfen: `curl -f http://127.0.0.1:8080/healthz` bzw. `havenmail-cli status`
5. Stichprobenartige Zustellungs- und Login-Tests

## Noch nicht umgesetzt

- S3-kompatibles Backup-Ziel
- Gestaffelte Retention (täglich/wöchentlich/monatlich)
- Einzel-Domain-/Benutzer-Restore (aktuell nur Vollständig-Restore)
- Automatisierter, periodischer Restore-Test in eine isolierte Sandbox
