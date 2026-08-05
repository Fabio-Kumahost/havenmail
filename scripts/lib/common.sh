#!/usr/bin/env bash
# Havenmail — gemeinsame Hilfsfunktionen für install.sh/update.sh/backup.sh/
# restore.sh/uninstall.sh: Pfade, Secrets, systemd-Unit-Verwaltung, Logging.
set -euo pipefail

# --- Standardpfade (siehe docs/architecture.md, Repository-Struktur) ---
HAVENMAIL_ETC_DIR="${HAVENMAIL_ETC_DIR:-/etc/havenmail}"
HAVENMAIL_STATE_DIR="${HAVENMAIL_STATE_DIR:-/var/lib/havenmail}"
HAVENMAIL_MAIL_DIR="${HAVENMAIL_MAIL_DIR:-/var/mail/havenmail}"
HAVENMAIL_LOG_DIR="${HAVENMAIL_LOG_DIR:-/var/log/havenmail}"
HAVENMAIL_ENV_FILE="${HAVENMAIL_ENV_FILE:-${HAVENMAIL_ETC_DIR}/havenmail.env}"
HAVENMAIL_REPO_DIR="${HAVENMAIL_REPO_DIR:-/opt/havenmail}"
HAVENMAIL_SYSTEM_USER="${HAVENMAIL_SYSTEM_USER:-havenmail}"
HAVENMAIL_SOURCE_REPO="${HAVENMAIL_SOURCE_REPO:-https://github.com/Fabio-Kumahost/havenmail.git}"

# Rust-Toolchain-Pfade IMMER exportieren, nicht nur innerhalb von
# havenmail_install_rust_toolchain: rustups cargo-Shim (verlinkt nach
# /usr/local/bin, siehe dort) braucht RUSTUP_HOME/CARGO_HOME in JEDEM
# Prozess, der cargo aufruft — nicht nur in dem, der die Toolchain
# ursprünglich installiert hat. Ohne dies schlägt z. B. `update.sh` in einer
# neuen Shell fehl, obwohl `command -v cargo` erfolgreich ist: "rustup could
# not choose a version of cargo to run, ... no default is configured"
# (in einem Debian-12-Testcontainer real aufgetreten).
export RUSTUP_HOME="${RUSTUP_HOME:-/opt/rustup}"
export CARGO_HOME="${CARGO_HOME:-/opt/cargo}"
export PATH="${CARGO_HOME}/bin:${PATH}"

havenmail_log() {
  echo "[havenmail] $*"
}

havenmail_err() {
  echo "[havenmail] Fehler: $*" >&2
}

# Erzeugt einen zufälligen, URL-sicheren String aus mindestens $1 Byte
# Zufall (Standard: 32). Nutzt /dev/urandom über openssl — keine eigene
# Zufallszahlenerzeugung.
havenmail_random_secret() {
  local bytes="${1:-32}"
  openssl rand -base64 "$bytes" | tr -d '=+/\n' | head -c "$((bytes * 4 / 3))"
}

# Erzeugt genau 32 Rohbyte, hex-kodiert (für HAVENMAIL_SECRETS_KEY, das als
# genau-32-Byte-AES-Schlüssel interpretiert wird — daher roh, nicht base64).
havenmail_random_key_hex32() {
  openssl rand -hex 32
}

havenmail_ensure_dirs() {
  install -d -m 0750 -o root -g root "$HAVENMAIL_ETC_DIR"
  install -d -m 0750 -o "$HAVENMAIL_SYSTEM_USER" -g "$HAVENMAIL_SYSTEM_USER" "$HAVENMAIL_STATE_DIR"
  install -d -m 0750 -o "$HAVENMAIL_SYSTEM_USER" -g "$HAVENMAIL_SYSTEM_USER" "$HAVENMAIL_MAIL_DIR"
  install -d -m 0750 -o "$HAVENMAIL_SYSTEM_USER" -g "$HAVENMAIL_SYSTEM_USER" "$HAVENMAIL_LOG_DIR"
}

havenmail_ensure_system_user() {
  if ! id "$HAVENMAIL_SYSTEM_USER" >/dev/null 2>&1; then
    useradd --system --home-dir "$HAVENMAIL_STATE_DIR" --shell /usr/sbin/nologin \
      --uid 5000 --user-group "$HAVENMAIL_SYSTEM_USER"
    havenmail_log "Systembenutzer '${HAVENMAIL_SYSTEM_USER}' angelegt (uid 5000)."
  fi
}

# Liest einen Wert aus der Env-Datei, falls vorhanden (für Idempotenz: bei
# wiederholter Installation bereits generierte Secrets wiederverwenden statt
# neue zu erzeugen und bestehende Logins/Verschlüsselung zu invalidieren).
havenmail_env_get() {
  local key="$1"
  if [[ -r "$HAVENMAIL_ENV_FILE" ]]; then
    grep -E "^${key}=" "$HAVENMAIL_ENV_FILE" | tail -n1 | cut -d= -f2-
  fi
}

havenmail_require_command() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    havenmail_err "Benötigtes Kommando '${cmd}' nicht gefunden."
    exit 1
  fi
}
