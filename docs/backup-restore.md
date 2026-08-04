# Backup & Restore

> **Status:** `backup.sh`/`restore.sh` sind aktuell Gerüst-Skripte; die vollständige Implementierung (Verschlüsselung, S3-Ziel, Restore-Test-Automatik) folgt in M5.

## Zielbild

- Konsistente Backups von: Maildaten (Maildir), PostgreSQL-Datenbank, Konfiguration (`config/`), Schlüsselmaterial (DKIM, TLS)
- Verschlüsselung der Backup-Archive (age/gpg) mit einem separat verwalteten Schlüssel
- Ziele: lokal (Standard) und optional S3-kompatibler Object Storage
- Aufbewahrungsregeln (Retention) konfigurierbar, Standard: 7 täglich / 4 wöchentlich / 6 monatlich
- `restore.sh` verlangt eine explizite Bestätigung, bevor vorhandene Daten überschrieben werden
- Wiederherstellung einzelner Domains/Benutzer, soweit technisch möglich (Maildir-Granularität)
- Automatisierter, periodischer Restore-Test in eine isolierte Sandbox-Umgebung

## Disaster-Recovery-Prozess (Zielbild)

1. Neuestes verifiziertes Backup identifizieren (`restore.sh --list`)
2. Wartungsmodus aktivieren
3. `restore.sh --target <backup-id>` mit expliziter Bestätigung ausführen
4. Dienste starten, Health-Checks prüfen
5. Stichprobenartige Zustellungs- und Login-Tests
6. Wartungsmodus deaktivieren

Details werden mit der Implementierung in M5 ergänzt.
