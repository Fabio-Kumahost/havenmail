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

# Erzeugt eine ZUFÄLLIGE Zeichenkette exakter Länge $1 aus reinen
# Alphanumerischen Zeichen (kein "=+/", die havenmail_random_secret nur
# herausfiltert, ohne die dadurch verkürzte Länge auszugleichen — für
# Werte mit harter Längenanforderung wie Roundcubes des_key, das exakt 24
# Zeichen lang sein muss, sonst schlägt Roundcubes eigene Prüfung fehl).
# Zieht großzügig mehr Rohbytes, als für $1 Zeichen nötig wären, damit nach
# dem Herausfiltern von "=+/" garantiert genug übrig bleibt.
havenmail_random_alnum() {
  local length="$1"
  openssl rand -base64 "$((length * 3))" | tr -d '=+/\n' | head -c "$length"
}

havenmail_ensure_dirs() {
  install -d -m 0750 -o root -g root "$HAVENMAIL_ETC_DIR"
  install -d -m 0750 -o "$HAVENMAIL_SYSTEM_USER" -g "$HAVENMAIL_SYSTEM_USER" "$HAVENMAIL_STATE_DIR"
  install -d -m 0750 -o "$HAVENMAIL_SYSTEM_USER" -g "$HAVENMAIL_SYSTEM_USER" "$HAVENMAIL_MAIL_DIR"
  install -d -m 0750 -o "$HAVENMAIL_SYSTEM_USER" -g "$HAVENMAIL_SYSTEM_USER" "$HAVENMAIL_LOG_DIR"
  # Eigenes Unterverzeichnis, NICHT $HAVENMAIL_ETC_DIR selbst beschreibbar
  # machen (das bleibt root:root 0750 — enthält havenmail.env mit
  # DB-Passwort/Secrets-Key). Private DKIM-Schlüssel + die von Rspamd
  # gelesenen selector_map/keys_map (siehe routes/dns.rs) landen hier;
  # havenmail-api.service bekommt dafür genau diesen Unterpfad in
  # ReadWritePaths, nicht das übergeordnete Verzeichnis.
  install -d -m 0750 -o "$HAVENMAIL_SYSTEM_USER" -g "$HAVENMAIL_SYSTEM_USER" "$HAVENMAIL_ETC_DIR/dkim"
}

havenmail_ensure_system_user() {
  if ! id "$HAVENMAIL_SYSTEM_USER" >/dev/null 2>&1; then
    useradd --system --home-dir "$HAVENMAIL_STATE_DIR" --shell /usr/sbin/nologin \
      --uid 5000 --user-group "$HAVENMAIL_SYSTEM_USER"
    havenmail_log "Systembenutzer '${HAVENMAIL_SYSTEM_USER}' angelegt (uid 5000)."
  fi
  # havenmail-cli snapshot-metrics liest /var/log/clamav/clamav.log für die
  # Virenfund-Statistik im Dashboard (siehe clamav_stats.rs) — das Log ist
  # per Debian-Default nur für root/Gruppe clamav lesbar.
  if getent group clamav >/dev/null 2>&1; then
    usermod -aG clamav "$HAVENMAIL_SYSTEM_USER"
  fi
  # `postqueue -p`/`-j` (Mail-Warteschlangen-Anzeige im Admin-Panel, siehe
  # routes/mail_queue.rs) verbindet sich mit Postfix' showq-Socket und
  # braucht dafür die Gruppe postdrop — normalerweise per Setgid-Bit auf
  # dem postqueue-Binary erlangt, aber havenmail-api.service setzt
  # NoNewPrivileges=true (blockiert jede Art von Rechte-Eskalation bei
  # execve, auch Setgid — live geprüft: "Connect to the Postfix showq
  # service: Permission denied"). Direkte Gruppenmitgliedschaft umgeht das
  # sauber, ganz ohne die Härtung zu lockern (der Prozess hat die Gruppe
  # dann schon beim Start, keine Rechte-Eskalation nötig).
  if getent group postdrop >/dev/null 2>&1; then
    usermod -aG postdrop "$HAVENMAIL_SYSTEM_USER"
  fi
  # havenmail-cli snapshot-metrics zählt gesendete/empfangene Mail über
  # `journalctl -u postfix` (siehe mail_flow.rs) — ohne systemd-journal-
  # Mitgliedschaft liefert journalctl für einen fremden Dienst keine
  # Einträge ("Users in groups 'adm', 'systemd-journal' can see all
  # messages", live geprüft). Reiner Lesezugriff, keine Rechte-Eskalation.
  if getent group systemd-journal >/dev/null 2>&1; then
    usermod -aG systemd-journal "$HAVENMAIL_SYSTEM_USER"
  fi
  # `sievec` (Kompilieren der Abwesenheitsnotiz-Skripte, siehe
  # routes/vacation.rs) lädt beim Start intern die volle Dovecot-Config
  # via doveconf, um Sieve-Erweiterungen aufzulösen — darunter
  # 10-auth-sql.conf, das die Datenbank-Zugangsdaten enthält und deshalb
  # nur root:dovecot 0640 lesbar ist. Ohne Gruppenmitgliedschaft schlägt
  # jeder Compile-Versuch mit "Permission denied" fehl (live geprüft).
  if getent group dovecot >/dev/null 2>&1; then
    usermod -aG dovecot "$HAVENMAIL_SYSTEM_USER"
  fi
}

# Umgekehrte Richtung zu den obigen Gruppenmitgliedschaften: hier braucht
# nicht der havenmail-Benutzer Zugriff auf einen fremden Dienst, sondern
# Rspamds eigener Systembenutzer (_rspamd) Lesezugriff auf die von der
# Control-Plane geschriebenen DKIM-Dateien (private Schlüssel +
# selector_map/keys_map, siehe routes/dns.rs) unter
# $HAVENMAIL_ETC_DIR/dkim (0750 havenmail:havenmail). Muss NACH
# havenmail_apt_packages laufen, da der Systembenutzer "_rspamd" erst mit
# der Paketinstallation von rspamd entsteht — vorher existiert er noch
# nicht. Live gefunden: ohne diese Mitgliedschaft scheitert JEDER
# Zugriffsversuch von Rspamds Worker-Prozessen mit "Permission denied"
# (rspamd_map_parse_backend), obwohl `rspamadm configtest` als root
# anstandslos durchläuft — die Karten wirken dadurch scheinbar korrekt
# konfiguriert, während die tatsächlich laufenden Worker-Prozesse die
# Dateien nie lesen konnten und DKIM-Signierung so nie stattfand.
havenmail_grant_rspamd_dkim_access() {
  if getent passwd _rspamd >/dev/null 2>&1 && getent group "$HAVENMAIL_SYSTEM_USER" >/dev/null 2>&1; then
    usermod -aG "$HAVENMAIL_SYSTEM_USER" _rspamd
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
