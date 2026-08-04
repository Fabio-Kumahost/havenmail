#!/usr/bin/env bash
#
# Havenmail — Deinstallations-Skript
#
# STATUS (M0): Gerüst, noch keine Implementierung. Geplantes Verhalten:
# Dienste stoppen/deaktivieren, Pakete entfernen; Nutzdaten (Mails, DB,
# Schlüssel) werden NUR nach ausdrücklicher, expliziter Bestätigung entfernt
# (Standard: Daten bleiben erhalten). Wird in M5 umgesetzt.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/lib/preflight.sh
source "${SCRIPT_DIR}/scripts/lib/preflight.sh"

havenmail_require_root

echo "Havenmail uninstall.sh ist noch nicht implementiert." >&2
exit 1
