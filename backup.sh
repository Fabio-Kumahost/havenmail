#!/usr/bin/env bash
#
# Havenmail — Backup-Skript
#
# STATUS (M0): Gerüst, noch keine Implementierung. Geplantes Verhalten
# (verschlüsselte, konsistente Backups von Maildaten/DB/Config/Schlüsseln,
# lokal oder S3-kompatibel) ist in docs/backup-restore.md beschrieben und
# wird in M5 umgesetzt.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/lib/preflight.sh
source "${SCRIPT_DIR}/scripts/lib/preflight.sh"

havenmail_require_root

echo "Havenmail backup.sh ist noch nicht implementiert (siehe docs/backup-restore.md)." >&2
exit 1
