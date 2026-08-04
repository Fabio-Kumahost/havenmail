#!/usr/bin/env bash
#
# Havenmail — Installer für Debian 12/13
#
# STATUS (M0): Gerüst. Führt Preflight-Checks aus und beendet sich danach
# kontrolliert, OHNE Pakete zu installieren oder Dienste zu verändern.
# Die vollständige Installationslogik (Pakete, Firewall, Datenverzeichnisse,
# TLS, DB-Migrationen, Dienststart) folgt in Meilenstein M5, siehe
# docs/architecture.md und docs/installation.md.
#
# Sichere Nutzung (empfohlen): Skript vor der Ausführung prüfen.
#   curl -fsSLo install.sh https://raw.githubusercontent.com/USERNAME/havenmail/main/install.sh
#   less install.sh
#   sudo bash install.sh
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/lib/preflight.sh
source "${SCRIPT_DIR}/scripts/lib/preflight.sh"

UNATTENDED=0
for arg in "$@"; do
  case "$arg" in
    --unattended) UNATTENDED=1 ;;
    --help|-h)
      cat <<'EOF'
Verwendung: install.sh [--unattended] [--version <tag>]

  --unattended   Keine interaktiven Rückfragen; erwartet HAVENMAIL_*-Umgebungsvariablen
  --version      Zu installierende Release-Version (Standard: aktuellstes signiertes Release)

STATUS: Dieser Installer ist ein Gerüst (M0) und installiert noch nichts.
Siehe docs/installation.md für den aktuellen Stand.
EOF
      exit 0
      ;;
  esac
done

echo "== Havenmail Installer (Gerüst, M0) =="
echo

havenmail_require_root
havenmail_require_debian
havenmail_check_arch
havenmail_check_min_ram_mb 2048
havenmail_check_min_disk_gb 20 /
havenmail_check_ports_free 25 587 465 143 993 443

echo
echo "Preflight-Checks abgeschlossen."
echo
echo "Dieser Installer befindet sich noch im Aufbau (Meilenstein M0/M5)."
echo "Es wurden KEINE Pakete installiert und KEINE Dienste verändert."
echo "Aktueller Implementierungsstand: siehe CHANGELOG.md und docs/architecture.md"
exit 0
