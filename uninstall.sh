#!/usr/bin/env bash
#
# Havenmail — Deinstallations-Skript
#
# STATUS (M5): Stoppt/deaktiviert die Havenmail-eigenen Dienstteile
# (Control-Plane-API, havenmail-vhost in nginx, die von install.sh
# geschriebenen Postfix-/Dovecot-/Rspamd-/Fail2ban-Fragmente) und meldet
# den Cleanup. Entfernt NICHT die apt-Pakete selbst (postgresql, nginx,
# postfix, …) — diese können von anderen Diensten auf demselben Host
# genutzt werden; das liegt in der Verantwortung des Administrators.
# Nutzdaten (Datenbank, Maildaten, /etc/havenmail inkl. Secrets) werden
# NUR mit --purge-data UND expliziter Bestätigung entfernt.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/lib/preflight.sh
source "${SCRIPT_DIR}/scripts/lib/preflight.sh"
# shellcheck source=scripts/lib/common.sh
source "${SCRIPT_DIR}/scripts/lib/common.sh"

PURGE_DATA=0
FORCE=0
for arg in "$@"; do
  case "$arg" in
    --purge-data) PURGE_DATA=1 ;;
    --force) FORCE=1 ;;
    --help|-h)
      cat <<'EOF'
Verwendung: uninstall.sh [--purge-data] [--force]

  --purge-data   Zusätzlich Datenbank, Maildaten und /etc/havenmail (inkl.
                 Secrets) endgültig löschen. Ohne diese Option bleiben alle
                 Nutzdaten erhalten.
  --force        Keine interaktive Bestätigung verlangen (für Automatisierung)

Entfernt NICHT die zugrunde liegenden apt-Pakete (postgresql, nginx,
postfix, dovecot, rspamd, clamav, fail2ban) — diese können von anderen
Diensten genutzt werden.
EOF
      exit 0
      ;;
  esac
done

havenmail_require_root

if [[ "$PURGE_DATA" -eq 1 && "$FORCE" -ne 1 ]]; then
  echo "ACHTUNG: --purge-data löscht die Datenbank, alle Maildaten und /etc/havenmail (inkl. Secrets) UNWIDERRUFLICH."
  echo "Vorher ein Backup erstellen: ./backup.sh"
  read -rp "Zum Fortfahren 'JA' eingeben: " confirmation
  if [[ "$confirmation" != "JA" ]]; then
    echo "Abgebrochen." >&2
    exit 1
  fi
fi

havenmail_log "Stoppe/deaktiviere Havenmail-Control-Plane…"
systemctl disable --now havenmail-api.service 2>/dev/null || true
rm -f /etc/systemd/system/havenmail-api.service
systemctl daemon-reload

havenmail_log "Entferne nginx-vhost…"
rm -f /etc/nginx/sites-enabled/havenmail.conf /etc/nginx/sites-available/havenmail.conf
systemctl reload nginx 2>/dev/null || true

havenmail_log "Entferne Postfix-/Dovecot-/Rspamd-/Fail2ban-Fragmente…"
rm -rf /etc/postfix/havenmail
rm -f /etc/dovecot/conf.d/10-mail.conf /etc/dovecot/conf.d/10-master.conf \
      /etc/dovecot/conf.d/10-ssl.conf /etc/dovecot/dovecot-sql.conf.ext
rm -f /etc/rspamd/local.d/antivirus.conf /etc/rspamd/local.d/dkim_signing.conf \
      /etc/rspamd/local.d/dmarc.conf /etc/rspamd/local.d/ratelimit.conf
rm -f /etc/fail2ban/filter.d/havenmail-postfix.conf /etc/fail2ban/filter.d/havenmail-dovecot.conf
havenmail_err "Postfix/Dovecot laufen jetzt ohne die entfernte Konfiguration weiter (main.cf wurde beim Install überschrieben, nicht auf einen Vorzustand zurückgesetzt) — vor dem nächsten Neustart dieser Dienste manuell prüfen."

if [[ "$PURGE_DATA" -eq 1 ]]; then
  havenmail_log "Lösche Datenbank und Nutzdaten (--purge-data)…"
  runuser -u postgres -- dropdb --if-exists havenmail
  runuser -u postgres -- psql -v ON_ERROR_STOP=1 --quiet -c "DROP ROLE IF EXISTS havenmail;" || true
  rm -rf "$HAVENMAIL_ETC_DIR" "$HAVENMAIL_STATE_DIR" "$HAVENMAIL_MAIL_DIR" "$HAVENMAIL_LOG_DIR"
  id "$HAVENMAIL_SYSTEM_USER" >/dev/null 2>&1 && userdel "$HAVENMAIL_SYSTEM_USER" 2>/dev/null || true
  havenmail_log "Nutzdaten gelöscht."
else
  havenmail_log "Nutzdaten bleiben erhalten: ${HAVENMAIL_ETC_DIR}, ${HAVENMAIL_STATE_DIR}, ${HAVENMAIL_MAIL_DIR} (Datenbank 'havenmail' unverändert)."
fi

echo
echo "== Deinstallation abgeschlossen =="
echo "apt-Pakete (postgresql, nginx, postfix, dovecot, rspamd, clamav, fail2ban, ufw) wurden NICHT entfernt."
