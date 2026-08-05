#!/usr/bin/env bash
#
# Havenmail — Backup-Skript
#
# STATUS (M5): Sichert Datenbank (pg_dump, custom format), /etc/havenmail
# (inkl. HAVENMAIL_SECRETS_KEY — ohne diesen Schlüssel sind die in der DB
# gespeicherten DKIM-Privatschlüssel nicht entschlüsselbar) und Maildaten
# in ein einziges Archiv. Optionale gpg-Verschlüsselung über
# HAVENMAIL_BACKUP_PASSPHRASE. Siehe docs/backup-restore.md für das
# vollständige Zielbild (S3-Ziel, gestaffelte Retention, automatisierte
# Restore-Tests sind noch nicht umgesetzt).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/lib/preflight.sh
source "${SCRIPT_DIR}/scripts/lib/preflight.sh"
# shellcheck source=scripts/lib/common.sh
source "${SCRIPT_DIR}/scripts/lib/common.sh"
# shellcheck source=scripts/lib/backup_steps.sh
source "${SCRIPT_DIR}/scripts/lib/backup_steps.sh"

for arg in "$@"; do
  case "$arg" in
    --list)
      havenmail_backup_list
      exit 0
      ;;
    --help|-h)
      cat <<'EOF'
Verwendung: backup.sh [--list]

  --list   Vorhandene Backups auflisten statt ein neues zu erstellen

Umgebungsvariablen:
  HAVENMAIL_BACKUP_DIR         Zielverzeichnis (Standard: /var/backups/havenmail)
  HAVENMAIL_BACKUP_RETENTION   Anzahl aufzubewahrender Backups (Standard: 14)
  HAVENMAIL_BACKUP_PASSPHRASE  gpg-Passphrase; wenn gesetzt, wird das Archiv verschlüsselt
EOF
      exit 0
      ;;
  esac
done

havenmail_require_root
havenmail_require_command pg_dump
havenmail_require_command tar

havenmail_backup_create
