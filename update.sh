#!/usr/bin/env bash
#
# Havenmail — Update-Skript
#
# STATUS (M0): Gerüst, noch keine Implementierung. Geplantes Verhalten
# (Zielversion prüfen, Backup vor Migration, Wartungsmodus, Rollback bei
# Fehler) ist in docs/upgrades.md beschrieben und wird in M5 umgesetzt.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/lib/preflight.sh
source "${SCRIPT_DIR}/scripts/lib/preflight.sh"

havenmail_require_root

echo "Havenmail update.sh ist noch nicht implementiert (siehe docs/upgrades.md)." >&2
exit 1
