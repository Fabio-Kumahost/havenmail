# Updates & Upgrades

> **Status:** `update.sh` ist aktuell ein Gerüst-Skript; die vollständige Implementierung folgt in M5.

## Zielbild

- `update.sh` prüft die Zielversion vor dem Update und lehnt nicht veröffentlichte/unsignierte Versionen ab
- Automatisches Backup vor riskanten Datenbankmigrationen
- Wartungsmodus während der Migration
- Rollback-Konzept: letztes Backup + vorherige Binärversion bleiben bis zum nächsten erfolgreichen Update erhalten
- Keine automatische Hauptversions-Aktualisierung ohne explizite Zustimmung (`update.sh --major` erforderlich)
- Status- und Diagnosebefehl (`havenmail-cli status`) vor und nach jedem Update

Details und konkrete Befehle werden mit der Implementierung in M5 ergänzt.
