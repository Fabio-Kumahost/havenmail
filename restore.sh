#!/usr/bin/env bash
#
# Havenmail — Restore-Skript
#
# STATUS (M5): Stellt ein mit backup.sh erstelltes Archiv wieder her
# (DB via pg_restore --clean, /etc/havenmail, Maildaten). Vorhandene Daten
# werden vor dem Überschreiben beiseite verschoben (*.before-restore-<ts>),
# nicht gelöscht. Verlangt eine explizite Bestätigung, außer mit --force.
# Einzel-Domain/-Benutzer-Restore und automatisierte Sandbox-Restore-Tests
# aus dem Zielbild (docs/backup-restore.md) sind noch nicht umgesetzt.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/lib/preflight.sh
source "${SCRIPT_DIR}/scripts/lib/preflight.sh"
# shellcheck source=scripts/lib/common.sh
source "${SCRIPT_DIR}/scripts/lib/common.sh"
# shellcheck source=scripts/lib/backup_steps.sh
source "${SCRIPT_DIR}/scripts/lib/backup_steps.sh"

TARGET=""
FORCE=0
for arg in "$@"; do
  case "$arg" in
    --list)
      havenmail_backup_list
      exit 0
      ;;
    --force) FORCE=1 ;;
    --target=*) TARGET="${arg#--target=}" ;;
    --help|-h)
      cat <<'EOF'
Verwendung: restore.sh --target <backup-datei> [--force]
       restore.sh --list

  --target   Pfad oder Dateiname (unterhalb von HAVENMAIL_BACKUP_DIR) des
             wiederherzustellenden Backups
  --list     Vorhandene Backups auflisten
  --force    Keine interaktive Bestätigung verlangen (für Automatisierung;
             HAVENMAIL_BACKUP_PASSPHRASE muss dann ggf. gesetzt sein)

ACHTUNG: Überschreibt die aktuelle Datenbank und Maildaten. Vorherige
Daten werden nach *.before-restore-<Zeitstempel> verschoben, nicht
gelöscht — nach erfolgreicher Prüfung manuell entfernen.
EOF
      exit 0
      ;;
  esac
done

havenmail_require_root
havenmail_require_command pg_restore
havenmail_require_command tar

if [[ -z "$TARGET" ]]; then
  echo "Fehler: --target <backup-datei> erforderlich (siehe --help, oder --list für vorhandene Backups)." >&2
  exit 1
fi

if [[ "$FORCE" -ne 1 ]]; then
  echo "ACHTUNG: Dies überschreibt die aktuelle Datenbank und Maildaten von ${HAVENMAIL_HOSTNAME:-diesem Server} mit dem Inhalt von '${TARGET}'."
  read -rp "Zum Fortfahren 'JA' eingeben: " confirmation
  if [[ "$confirmation" != "JA" ]]; then
    echo "Abgebrochen." >&2
    exit 1
  fi
fi

havenmail_backup_restore "$TARGET"
