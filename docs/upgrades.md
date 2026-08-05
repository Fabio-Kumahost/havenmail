# Updates & Upgrades

> **Status (M5):** `update.sh` ist implementiert. Signierte-Release-Prüfung aus dem ursprünglichen Zielbild ist noch nicht umgesetzt — `--version` akzeptiert jede Git-Ref des konfigurierten Quell-Repos.

## Verwendung

```bash
sudo ./update.sh                       # aktualisiert auf 'main'
sudo ./update.sh --version v1.2.0
sudo ./update.sh --version v2.0.0 --major   # nötig bei Hauptversionssprung
sudo ./update.sh --skip-backup         # nicht empfohlen
```

## Ablauf

1. Aktuelle Ref ermitteln, Hauptversionssprung ohne `--major` ablehnen
2. Automatisches Backup (`backup.sh`-Logik) — außer mit `--skip-backup`
3. Aktuelle Release-Binaries sichern (`/var/lib/havenmail/previous-release/`) für Rollback
4. Wartungsmodus: Control-Plane-API wird gestoppt. **SMTP/IMAP-Zustellung über Postfix/Dovecot läuft währenddessen weiter** — sie hängen nicht vom API-Prozess ab, nur von der Datenbank
5. Ziel-Ref auschecken, Skript startet sich aus der neuen Ref neu (verhindert, dass `git checkout` die gerade laufende Skriptdatei unter sich selbst verändert)
6. Backend/Frontend neu bauen, Konfiguration neu rendern/deployen (Postfix/Dovecot/Rspamd/Fail2ban/nginx), systemd-Unit neu installieren (Migrationen laufen automatisch beim API-Start)
7. Wartungsmodus beenden, alle Dienste neu starten, Health-Check
8. Bei einem Fehler in Schritt 6–7: automatischer Trap rollt die Release-Binaries zurück und startet die API erneut. Bei Datenbankmigrationen zusätzlich `restore.sh --target <Backup aus Schritt 2>` nötig — der Binary-Rollback allein macht Schema-Änderungen nicht rückgängig

## Status- und Diagnosebefehl

```bash
havenmail-cli status   # /readyz der Control-Plane (DB-Verbindung geprüft)
```

Vor und nach jedem Update sinnvoll, um den Erfolg zu verifizieren.

## Noch nicht umgesetzt

- Prüfung/Ablehnung nicht veröffentlichter oder unsignierter Versionen — `update.sh` vertraut aktuell jeder erreichbaren Git-Ref
