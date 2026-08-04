#!/usr/bin/env bash
#
# Havenmail — Restore-Skript
#
# STATUS (M0): Gerüst, noch keine Implementierung. Geplantes Verhalten
# (Wiederherstellung aus verschlüsseltem Backup, KEIN Überschreiben ohne
# explizite Bestätigung, Einzel-Domain/-Benutzer-Restore) ist in
# docs/backup-restore.md beschrieben und wird in M5 umgesetzt.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/lib/preflight.sh
source "${SCRIPT_DIR}/scripts/lib/preflight.sh"

havenmail_require_root

echo "Havenmail restore.sh ist noch nicht implementiert (siehe docs/backup-restore.md)." >&2
exit 1
