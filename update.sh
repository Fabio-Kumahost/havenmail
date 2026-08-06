#!/usr/bin/env bash
#
# Havenmail — Update-Skript
#
# STATUS (M5): Holt eine Ziel-Ref, baut neu, rendert/deployed Konfiguration
# neu und startet die Dienste — mit automatischem Backup davor und
# Wartungsmodus (Control-Plane-API) währenddessen. SMTP/IMAP-Zustellung
# über Postfix/Dovecot läuft während des Updates weiter (sie hängen nicht
# von der API ab). Signierte-Release-Prüfung aus dem Zielbild
# (docs/upgrades.md) ist noch nicht umgesetzt — `--version` akzeptiert
# jede Git-Ref des konfigurierten Quell-Repos.
#
# Läuft in zwei Phasen mit einem exec dazwischen (wie install.sh):
# Phase 1 checkt die Ziel-Ref aus; Phase 2 baut/deployed von dort. Ohne
# diesen Zwischenschritt würde `git checkout` die gerade laufende
# update.sh-Datei unter sich selbst verändern — bash liest Skriptdateien
# nicht atomar ein, das Verhalten danach wäre undefiniert.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/lib/preflight.sh
source "${SCRIPT_DIR}/scripts/lib/preflight.sh"
# shellcheck source=scripts/lib/common.sh
source "${SCRIPT_DIR}/scripts/lib/common.sh"
# shellcheck source=scripts/lib/install_steps.sh
source "${SCRIPT_DIR}/scripts/lib/install_steps.sh"
# shellcheck source=scripts/lib/backup_steps.sh
source "${SCRIPT_DIR}/scripts/lib/backup_steps.sh"

HAVENMAIL_REPO_DIR="$SCRIPT_DIR"
export HAVENMAIL_REPO_DIR

TARGET_REF="${HAVENMAIL_UPDATE_TARGET_REF:-main}"
ALLOW_MAJOR=0
SKIP_BACKUP=0
# Sowohl "--version=wert" als auch "--version wert" akzeptieren (echter
# Bugfund beim End-to-End-Test von restore.sh — dieselbe Inkonsistenz
# zwischen --help-Text und Parser bestand hier auch).
while [[ $# -gt 0 ]]; do
  case "$1" in
    --version=*) TARGET_REF="${1#--version=}"; shift ;;
    --version) TARGET_REF="${2:-main}"; shift 2 ;;
    --major) ALLOW_MAJOR=1; shift ;;
    --skip-backup) SKIP_BACKUP=1; shift ;;
    --help|-h)
      cat <<'EOF'
Verwendung: update.sh [--version <ref>] [--major] [--skip-backup]

  --version      Ziel-Ref (Branch/Tag) im Quell-Repo (Standard: main)
  --major        Erforderlich, wenn sich die Hauptversion ändert (vX.y.z -> vX+1.*)
  --skip-backup  Kein automatisches Backup vor der Migration erstellen (nicht empfohlen)
EOF
      exit 0
      ;;
    *) shift ;;
  esac
done

havenmail_require_root
havenmail_require_command git

if [[ "${HAVENMAIL_UPDATE_PHASE:-1}" -eq 1 ]]; then
  CURRENT_REF="$(git -C "$HAVENMAIL_REPO_DIR" describe --tags --exact-match 2>/dev/null || git -C "$HAVENMAIL_REPO_DIR" rev-parse --short HEAD)"

  # Hauptversions-Sprung nur mit ausdrücklicher Zustimmung (docs/upgrades.md).
  current_major="${CURRENT_REF#v}"; current_major="${current_major%%.*}"
  target_major="${TARGET_REF#v}"; target_major="${target_major%%.*}"
  if [[ "$CURRENT_REF" =~ ^v[0-9] && "$TARGET_REF" =~ ^v[0-9] && \
        "$current_major" != "$target_major" && "$ALLOW_MAJOR" -ne 1 ]]; then
    echo "Fehler: ${CURRENT_REF} -> ${TARGET_REF} ist ein Hauptversionssprung. Mit --major bestätigen." >&2
    exit 1
  fi

  echo "== Havenmail Update: ${CURRENT_REF} -> ${TARGET_REF} =="

  if [[ "$SKIP_BACKUP" -ne 1 ]]; then
    havenmail_log "Erstelle Backup vor der Migration…"
    havenmail_backup_create
  else
    havenmail_err "Backup übersprungen (--skip-backup) — bei einem fehlgeschlagenen Update gibt es keinen automatischen Rollback-Datenstand."
  fi

  havenmail_log "Sichere aktuelle Release-Binaries für Rollback…"
  install -d -m 0700 "${HAVENMAIL_STATE_DIR}/previous-release"
  cp -f "${HAVENMAIL_REPO_DIR}/backend/target/release/havenmail-api" \
        "${HAVENMAIL_STATE_DIR}/previous-release/" 2>/dev/null || true
  cp -f "${HAVENMAIL_REPO_DIR}/backend/target/release/havenmail-cli" \
        "${HAVENMAIL_STATE_DIR}/previous-release/" 2>/dev/null || true

  havenmail_log "Aktiviere Wartungsmodus (Control-Plane-API gestoppt; SMTP/IMAP bleiben erreichbar)…"
  systemctl stop havenmail-api.service 2>/dev/null || true

  havenmail_fetch_source "$TARGET_REF"

  havenmail_log "Starte Update-Skript aus der neuen Ref neu (Phase 2)…"
  HAVENMAIL_UPDATE_PHASE=2 HAVENMAIL_UPDATE_TARGET_REF="$TARGET_REF" \
    exec bash "${HAVENMAIL_REPO_DIR}/update.sh"
fi

# --- Phase 2: läuft bereits aus der ausgecheckten Ziel-Ref ---
rollback() {
  havenmail_err "Update fehlgeschlagen — rolle Binaries zurück und starte Dienste neu."
  cp -f "${HAVENMAIL_STATE_DIR}/previous-release/havenmail-api" \
        "${HAVENMAIL_REPO_DIR}/backend/target/release/havenmail-api" 2>/dev/null || true
  cp -f "${HAVENMAIL_STATE_DIR}/previous-release/havenmail-cli" \
        "${HAVENMAIL_REPO_DIR}/backend/target/release/havenmail-cli" 2>/dev/null || true
  systemctl start havenmail-api.service 2>/dev/null || true
  havenmail_err "Binary-Rollback durchgeführt. Bei Datenbankmigrationen ggf. zusätzlich 'restore.sh --target <letztes Backup>' ausführen."
}
trap rollback ERR

havenmail_build_backend
havenmail_build_frontend
havenmail_render_configs

# Idempotent — legt bei einer bereits laufenden Installation nichts neu an,
# holt aber die $HAVENMAIL_ETC_DIR/dkim-Berechtigungen (Rspamd-Zugriff auf
# DKIM-Schlüssel/-Maps, siehe common.sh) für Bestandsinstallationen nach,
# die vor Einführung der DKIM-Rotation aktualisiert werden.
havenmail_ensure_dirs
havenmail_grant_rspamd_dkim_access

havenmail_deploy_mail_configs
havenmail_deploy_nginx_full
havenmail_install_systemd_units

havenmail_log "Beende Wartungsmodus…"
systemctl start havenmail-api.service
systemctl restart postfix dovecot rspamd nginx

havenmail_verify_health
trap - ERR

echo
echo "== Update abgeschlossen: jetzt auf ${TARGET_REF} =="
echo "Vorherige Release-Binaries für Rollback unter ${HAVENMAIL_STATE_DIR}/previous-release/"
