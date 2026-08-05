#!/usr/bin/env bash
# Havenmail — Backup-/Restore-Bausteine, genutzt von backup.sh, restore.sh
# und update.sh (automatisches Backup vor riskanten Migrationen).
# Siehe docs/backup-restore.md für das Zielbild.
set -euo pipefail

HAVENMAIL_BACKUP_DIR="${HAVENMAIL_BACKUP_DIR:-/var/backups/havenmail}"
# Anzahl aufzubewahrender Backups (einfache Anzahl-basierte Retention;
# Tages-/Wochen-/Monats-Staffelung aus dem Zielbild ist noch nicht
# umgesetzt, siehe docs/backup-restore.md).
HAVENMAIL_BACKUP_RETENTION="${HAVENMAIL_BACKUP_RETENTION:-14}"

# Erstellt ein konsistentes Backup (DB-Dump + /etc/havenmail inkl.
# HAVENMAIL_SECRETS_KEY + Maildaten) als ein einziges Archiv. Verschlüsselt
# mit gpg (symmetrisch), falls HAVENMAIL_BACKUP_PASSPHRASE gesetzt ist —
# ansonsten liegt das Archiv unverschlüsselt vor (Warnung wird ausgegeben,
# siehe docs/backup-restore.md: Verschlüsselung ist Zielbild, aber ohne
# verwalteten Schlüssel kann sie nicht automatisch erzwungen werden).
havenmail_backup_create() {
  local ts stage_dir archive db_url
  ts="$(date -u +%Y%m%dT%H%M%SZ)"
  stage_dir="$(mktemp -d)"
  trap 'rm -rf "$stage_dir"' RETURN

  install -d -m 0700 "$HAVENMAIL_BACKUP_DIR"

  db_url="$(havenmail_env_get DATABASE_URL)"
  havenmail_log "Sichere Datenbank (pg_dump, custom format)…"
  pg_dump --format=custom --file="${stage_dir}/db.dump" "$db_url"

  havenmail_log "Erstelle Backup-Archiv…"
  archive="${HAVENMAIL_BACKUP_DIR}/havenmail-${ts}.tar.gz"
  tar -czf "$archive" \
    -C "$stage_dir" db.dump \
    -C / "${HAVENMAIL_ETC_DIR#/}" \
    -C / "${HAVENMAIL_MAIL_DIR#/}"

  if [[ -n "${HAVENMAIL_BACKUP_PASSPHRASE:-}" ]]; then
    havenmail_log "Verschlüssele Archiv (gpg, symmetrisch)…"
    gpg --batch --yes --passphrase "$HAVENMAIL_BACKUP_PASSPHRASE" \
      --cipher-algo AES256 --symmetric --output "${archive}.gpg" "$archive"
    rm -f "$archive"
    archive="${archive}.gpg"
  else
    havenmail_err "HAVENMAIL_BACKUP_PASSPHRASE nicht gesetzt — Backup liegt UNVERSCHLÜSSELT vor (${archive}). Für Produktivbetrieb setzen."
  fi

  chmod 0600 "$archive"
  havenmail_log "Backup erstellt: ${archive}"
  echo "$archive"

  havenmail_backup_apply_retention
}

# Löscht die ältesten Archive, bis höchstens HAVENMAIL_BACKUP_RETENTION
# übrig sind.
havenmail_backup_apply_retention() {
  [[ -d "$HAVENMAIL_BACKUP_DIR" ]] || return 0
  local -a archives
  mapfile -t archives < <(find "$HAVENMAIL_BACKUP_DIR" -maxdepth 1 -type f -name 'havenmail-*.tar.gz*' | sort)
  local excess=$(( ${#archives[@]} - HAVENMAIL_BACKUP_RETENTION ))
  (( excess > 0 )) || return 0
  local i
  for (( i = 0; i < excess; i++ )); do
    havenmail_log "Entferne altes Backup (Retention ${HAVENMAIL_BACKUP_RETENTION}): ${archives[$i]}"
    rm -f "${archives[$i]}"
  done
}

havenmail_backup_list() {
  install -d -m 0700 "$HAVENMAIL_BACKUP_DIR"
  find "$HAVENMAIL_BACKUP_DIR" -maxdepth 1 -type f -name 'havenmail-*.tar.gz*' | sort
}

# Stellt ein Backup wieder her. `$1` ist entweder ein voller Pfad oder ein
# Dateiname unterhalb von HAVENMAIL_BACKUP_DIR. Verschiebt vorhandene Daten
# statt sie zu löschen (".before-restore-<ts>"), damit ein fehlgeschlagener
# Restore nicht zusätzlich Daten vernichtet.
havenmail_backup_restore() {
  local target="$1"
  [[ -f "$target" ]] || target="${HAVENMAIL_BACKUP_DIR}/${target}"
  [[ -f "$target" ]] || { havenmail_err "Backup nicht gefunden: $1"; exit 1; }

  local stage_dir ts db_url
  stage_dir="$(mktemp -d)"
  trap 'rm -rf "$stage_dir"' RETURN
  ts="$(date -u +%Y%m%dT%H%M%SZ)"

  local archive="$target"
  if [[ "$target" == *.gpg ]]; then
    havenmail_log "Entschlüssele Archiv…"
    local passphrase="${HAVENMAIL_BACKUP_PASSPHRASE:-}"
    if [[ -z "$passphrase" ]]; then
      read -rsp "GPG-Passphrase für Backup: " passphrase
      echo
    fi
    archive="${stage_dir}/decrypted.tar.gz"
    gpg --batch --yes --passphrase "$passphrase" --decrypt --output "$archive" "$target"
  fi

  havenmail_log "Entpacke Backup…"
  tar -xzf "$archive" -C "$stage_dir"

  havenmail_log "Stoppe Dienste vor der Wiederherstellung…"
  systemctl stop havenmail-api postfix dovecot rspamd 2>/dev/null || true

  db_url="$(havenmail_env_get DATABASE_URL)"
  havenmail_log "Stelle Datenbank wieder her (pg_restore --clean)…"
  pg_restore --format=custom --clean --if-exists --no-owner \
    --dbname="$db_url" "${stage_dir}/db.dump"

  if [[ -d "$HAVENMAIL_ETC_DIR" ]]; then
    mv "$HAVENMAIL_ETC_DIR" "${HAVENMAIL_ETC_DIR}.before-restore-${ts}"
  fi
  install -d -m 0750 -o root -g "$HAVENMAIL_SYSTEM_USER" "$(dirname "$HAVENMAIL_ETC_DIR")"
  cp -a "${stage_dir}${HAVENMAIL_ETC_DIR}" "$HAVENMAIL_ETC_DIR"

  if [[ -d "$HAVENMAIL_MAIL_DIR" ]]; then
    mv "$HAVENMAIL_MAIL_DIR" "${HAVENMAIL_MAIL_DIR}.before-restore-${ts}"
  fi
  install -d -m 0750 -o "$HAVENMAIL_SYSTEM_USER" -g "$HAVENMAIL_SYSTEM_USER" "$(dirname "$HAVENMAIL_MAIL_DIR")"
  cp -a "${stage_dir}${HAVENMAIL_MAIL_DIR}" "$HAVENMAIL_MAIL_DIR"
  chown -R "$HAVENMAIL_SYSTEM_USER":"$HAVENMAIL_SYSTEM_USER" "$HAVENMAIL_MAIL_DIR"

  havenmail_log "Starte Dienste neu…"
  systemctl start havenmail-api postfix dovecot rspamd

  havenmail_log "Restore abgeschlossen. Vorherige Daten liegen unter *.before-restore-${ts} und können nach Prüfung gelöscht werden."
}
