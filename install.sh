#!/usr/bin/env bash
#
# Havenmail — Installer für Debian 12/13
#
# STATUS (M5): Führt eine vollständige Erstinstallation durch — Pakete,
# Systembenutzer/-verzeichnisse, Postfix/Dovecot/Rspamd/ClamAV/Fail2ban-
# Konfiguration, PostgreSQL-Rolle/DB, TLS-Zertifikat, Control-Plane-Build
# und -Dienst, sowie den ersten super_admin-Zugang. Siehe
# docs/architecture.md und docs/installation.md.
#
# Sichere Nutzung (empfohlen): Skript vor der Ausführung prüfen.
#   curl -fsSLo install.sh https://raw.githubusercontent.com/USERNAME/havenmail/main/install.sh
#   less install.sh
#   sudo bash install.sh
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Für den Self-Bootstrap-Re-Exec weiter unten (`exec bash install.sh "$@"`)
# — die Parser-Schleife darunter konsumiert "$@" per shift, das Original
# bleibt hier erhalten.
ORIGINAL_ARGS=("$@")

UNATTENDED=0
VERSION_REF="main"
# Sowohl "--version=wert" als auch "--version wert" akzeptieren (echter
# Bugfund beim End-to-End-Test: --help dokumentierte die Leerzeichen-Form,
# geparst wurde aber nur "=").
while [[ $# -gt 0 ]]; do
  case "$1" in
    --unattended) UNATTENDED=1; shift ;;
    --version=*) VERSION_REF="${1#--version=}"; shift ;;
    --version) VERSION_REF="${2:-main}"; shift 2 ;;
    --help|-h)
      cat <<'EOF'
Verwendung: install.sh [--unattended] [--version <tag>]

  --unattended   Keine interaktiven Rückfragen; erwartet HAVENMAIL_*-Umgebungsvariablen
                 (HAVENMAIL_DOMAIN, HAVENMAIL_HOSTNAME, HAVENMAIL_ADMIN_EMAIL,
                 HAVENMAIL_TIMEZONE)
  --version      Zu installierende Quell-Ref (Branch/Tag, Standard: main)
EOF
      exit 0
      ;;
    *) shift ;;
  esac
done

# --- Preflight (unabhängig davon, ob wir bereits im Repo laufen) ---
if [[ -r "${SCRIPT_DIR}/scripts/lib/preflight.sh" ]]; then
  # shellcheck source=scripts/lib/preflight.sh
  source "${SCRIPT_DIR}/scripts/lib/preflight.sh"
else
  echo "Fehler: scripts/lib/preflight.sh nicht gefunden neben install.sh." >&2
  exit 1
fi

echo "== Havenmail Installer =="
echo

havenmail_require_root
havenmail_require_debian
havenmail_check_arch
havenmail_check_min_ram_mb 2048
havenmail_check_min_disk_gb 20 /
havenmail_check_ports_free 25 587 465 143 993 443
echo

# --- Self-Bootstrap: entweder wir laufen bereits aus einem vollständigen
# Checkout (scripts/lib komplett vorhanden, z. B. `git clone` + `sudo bash
# install.sh`), oder es wurde nur diese eine Datei per curl geladen — dann
# zuerst den vollen Quellcode holen und den Installer von dort neu starten.
if [[ -r "${SCRIPT_DIR}/scripts/lib/common.sh" && -r "${SCRIPT_DIR}/scripts/lib/install_steps.sh" ]]; then
  HAVENMAIL_REPO_DIR="$SCRIPT_DIR"
  export HAVENMAIL_REPO_DIR
  # shellcheck source=scripts/lib/common.sh
  source "${SCRIPT_DIR}/scripts/lib/common.sh"
else
  # shellcheck source=scripts/lib/common.sh
  # (common.sh selbst existiert bei Single-File-Download noch nicht lokal —
  # Standardwerte kommen daher direkt aus den Env-Defaults der Variable.)
  HAVENMAIL_REPO_DIR="${HAVENMAIL_REPO_DIR:-/opt/havenmail}"
  echo "Hole vollständigen Quellcode nach ${HAVENMAIL_REPO_DIR}…"
  havenmail_require_command git
  if [[ -d "${HAVENMAIL_REPO_DIR}/.git" ]]; then
    git -C "$HAVENMAIL_REPO_DIR" fetch --quiet origin
    git -C "$HAVENMAIL_REPO_DIR" checkout --quiet "$VERSION_REF"
  else
    git clone --quiet --branch "$VERSION_REF" \
      "${HAVENMAIL_SOURCE_REPO:-https://github.com/USERNAME/havenmail.git}" \
      "$HAVENMAIL_REPO_DIR"
  fi
  echo "Starte Installer aus dem vollständigen Checkout neu…"
  exec bash "${HAVENMAIL_REPO_DIR}/install.sh" "${ORIGINAL_ARGS[@]}"
fi

# shellcheck source=scripts/lib/install_steps.sh
source "${SCRIPT_DIR}/scripts/lib/install_steps.sh"

# --- Konfigurationswerte einsammeln ---
if [[ "$UNATTENDED" -eq 1 ]]; then
  : "${HAVENMAIL_DOMAIN:?--unattended benötigt HAVENMAIL_DOMAIN}"
  : "${HAVENMAIL_HOSTNAME:?--unattended benötigt HAVENMAIL_HOSTNAME}"
  : "${HAVENMAIL_ADMIN_EMAIL:?--unattended benötigt HAVENMAIL_ADMIN_EMAIL}"
  : "${HAVENMAIL_TIMEZONE:?--unattended benötigt HAVENMAIL_TIMEZONE}"
else
  read -rp "Mail-Domain (z. B. example.org): " HAVENMAIL_DOMAIN
  read -rp "Mail-Hostname (z. B. mail.example.org): " HAVENMAIL_HOSTNAME
  read -rp "Admin-E-Mail für TLS-Benachrichtigungen (z. B. admin@example.org): " HAVENMAIL_ADMIN_EMAIL
  read -rp "Zeitzone [Europe/Berlin]: " HAVENMAIL_TIMEZONE
  HAVENMAIL_TIMEZONE="${HAVENMAIL_TIMEZONE:-Europe/Berlin}"
fi
ADMIN_LOCAL_PART="${HAVENMAIL_ADMIN_EMAIL%%@*}"

echo
echo "Installiere Havenmail für Domain '${HAVENMAIL_DOMAIN}' (Hostname: ${HAVENMAIL_HOSTNAME})…"
echo

havenmail_ensure_system_user
havenmail_ensure_dirs

# Pakete VOR der Env-Datei: havenmail_write_env_file generiert Secrets über
# `openssl` (common.sh, havenmail_random_secret/havenmail_random_key_hex32)
# — auf einem frischen Minimal-Debian ist openssl nicht garantiert
# vorinstalliert (in einem Debian-12-Testcontainer ohne Vorinstallation
# schlug genau das fehl: "openssl: command not found").
havenmail_apt_packages
havenmail_install_rust_toolchain
havenmail_install_node

havenmail_write_env_file "$HAVENMAIL_DOMAIN" "$HAVENMAIL_HOSTNAME" "$HAVENMAIL_ADMIN_EMAIL" "$HAVENMAIL_TIMEZONE"

havenmail_configure_postgres

# Build vor dem Config-Rendering: render-configs/bootstrap-admin sind
# CLI-Befehle aus dem gerade gebauten Binary, das Frontend muss existieren,
# bevor nginx darauf als Root-Verzeichnis verweist.
havenmail_build_backend
havenmail_build_frontend
havenmail_render_configs

# TLS erst NACH einem laufenden, aber noch zertifikatslosen nginx (Port-80-
# Übergangs-vhost) anfordern — siehe Kommentare in install_steps.sh für die
# Begründung des zweiphasigen Rollouts.
havenmail_deploy_nginx_bootstrap
havenmail_provision_tls "$HAVENMAIL_HOSTNAME" "$HAVENMAIL_ADMIN_EMAIL"

havenmail_deploy_mail_configs
havenmail_deploy_nginx_full
havenmail_configure_firewall

havenmail_install_systemd_units
havenmail_start_services
havenmail_verify_health

havenmail_bootstrap_admin "$HAVENMAIL_DOMAIN" "$ADMIN_LOCAL_PART"

echo
echo "== Installation abgeschlossen =="
echo "DNS-Einträge gemäß docs/dns-setup.md setzen, dann Admin-Oberfläche"
echo "unter https://${HAVENMAIL_HOSTNAME}/ aufrufen."
echo "Erstzugangsdaten: ${HAVENMAIL_ETC_DIR}/initial-admin-credentials (nur root/${HAVENMAIL_SYSTEM_USER} lesbar)."
