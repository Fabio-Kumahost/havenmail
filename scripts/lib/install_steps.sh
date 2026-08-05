#!/usr/bin/env bash
# Havenmail — Installationsschritte, ausgelagert aus install.sh für
# Lesbarkeit und Wiederverwendung durch update.sh (Paketaktualisierung,
# Neubau, Neustart nutzen dieselben Bausteine).
set -euo pipefail

havenmail_apt_packages() {
  # Kernpakete für die orchestrierten Mail-Engines (siehe docs/architecture.md,
  # Komponenten & Lizenzen) sowie Build-Toolchain für die Control-Plane.
  # Kein `apt-get upgrade` — nur Installation der benötigten Pakete, um
  # bestehende Systempakete nicht anzufassen.
  local packages=(
    postgresql
    postfix
    postfix-pgsql
    dovecot-core
    dovecot-imapd
    dovecot-lmtpd
    dovecot-pgsql
    dovecot-sieve
    dovecot-managesieved
    rspamd
    clamav
    clamav-daemon
    nginx
    certbot
    python3-certbot-nginx
    fail2ban
    ufw
    git
    curl
    build-essential
    pkg-config
    libssl-dev
    ca-certificates
  )
  havenmail_log "Installiere Systempakete (apt-get install)…"
  DEBIAN_FRONTEND=noninteractive apt-get update -qq
  DEBIAN_FRONTEND=noninteractive apt-get install -y -qq "${packages[@]}"
}

havenmail_install_rust_toolchain() {
  # Prebuilt-Release-Binaries sind für dieses frühe Projektstadium noch
  # nicht verfügbar (siehe docs/installation.md) — der Installer baut daher
  # aus dem Quellcode. Rustup wird systemweit unter /opt/rustup installiert,
  # damit der Build auch unter dem Systembenutzer reproduzierbar ist.
  if command -v cargo >/dev/null 2>&1; then
    return 0
  fi
  havenmail_log "Installiere Rust-Toolchain (rustup, nur für den Build benötigt)…"
  export RUSTUP_HOME=/opt/rustup
  export CARGO_HOME=/opt/cargo
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- -y --profile minimal --default-toolchain stable
  # shellcheck disable=SC1091
  . /opt/cargo/env
  ln -sf /opt/cargo/bin/cargo /usr/local/bin/cargo
  ln -sf /opt/cargo/bin/rustc /usr/local/bin/rustc
}

havenmail_install_node() {
  if command -v node >/dev/null 2>&1; then
    return 0
  fi
  havenmail_log "Installiere Node.js (nur für den Frontend-Build benötigt)…"
  DEBIAN_FRONTEND=noninteractive apt-get install -y -qq nodejs npm
}

havenmail_fetch_source() {
  local ref="${1:-main}"
  if [[ -d "${HAVENMAIL_REPO_DIR}/.git" ]]; then
    havenmail_log "Havenmail-Quellcode bereits vorhanden — aktualisiere auf ${ref}…"
    git -C "$HAVENMAIL_REPO_DIR" fetch --quiet origin
    git -C "$HAVENMAIL_REPO_DIR" checkout --quiet "$ref"
    git -C "$HAVENMAIL_REPO_DIR" reset --quiet --hard "origin/${ref}" 2>/dev/null || true
  else
    havenmail_log "Klone Havenmail-Quellcode (${ref})…"
    git clone --quiet --branch "$ref" "$HAVENMAIL_SOURCE_REPO" "$HAVENMAIL_REPO_DIR"
  fi
}

havenmail_build_backend() {
  havenmail_log "Baue Control-Plane (Rust, Release-Profil — kann einige Minuten dauern)…"
  # shellcheck disable=SC1091
  [[ -f /opt/cargo/env ]] && . /opt/cargo/env
  ( cd "${HAVENMAIL_REPO_DIR}/backend" && cargo build --release --quiet )
}

havenmail_build_frontend() {
  havenmail_log "Baue Web-Oberfläche (npm run build)…"
  ( cd "${HAVENMAIL_REPO_DIR}/frontend" && npm ci --silent && npm run build --silent )
}

havenmail_configure_firewall() {
  havenmail_log "Konfiguriere Firewall (ufw) — nur benötigte Ports…"
  ufw --force enable >/dev/null
  for port in 22/tcp 25/tcp 587/tcp 465/tcp 143/tcp 993/tcp 443/tcp 80/tcp; do
    ufw allow "$port" >/dev/null
  done
  # ManageSieve (4190) bewusst NICHT öffentlich freigegeben (siehe
  # docs/architecture.md, Portübersicht: "standardmäßig nur localhost/VPN").
}

# Schreibt die Env-Datei idempotent: bereits gesetzte Secrets werden
# wiederverwendet (siehe havenmail_env_get in common.sh), nur fehlende
# Werte werden neu generiert. So zerstört ein erneuter install.sh-Lauf
# keine bestehenden Logins oder verschlüsselten DKIM-Schlüssel.
havenmail_write_env_file() {
  local domain="$1" hostname="$2" admin_email="$3" timezone="$4"

  local db_password jwt_key secrets_key
  db_password="$(havenmail_env_get HAVENMAIL_DB_PASSWORD)"
  [[ -z "$db_password" ]] && db_password="$(havenmail_random_secret 32)"
  jwt_key="$(havenmail_env_get HAVENMAIL_JWT_SIGNING_KEY)"
  [[ -z "$jwt_key" ]] && jwt_key="$(havenmail_random_secret 48)"
  secrets_key="$(havenmail_env_get HAVENMAIL_SECRETS_KEY)"
  [[ -z "$secrets_key" ]] && secrets_key="$(havenmail_random_key_hex32 | cut -c1-32)"

  install -d -m 0750 -o root -g "$HAVENMAIL_SYSTEM_USER" "$HAVENMAIL_ETC_DIR"
  cat > "$HAVENMAIL_ENV_FILE" <<EOF
# Von install.sh generiert — enthält Geheimnisse, nicht ins Git-Repo aufnehmen.
HAVENMAIL_DOMAIN=${domain}
HAVENMAIL_HOSTNAME=${hostname}
HAVENMAIL_ADMIN_EMAIL=${admin_email}
HAVENMAIL_TIMEZONE=${timezone}

DATABASE_URL=postgres://havenmail:${db_password}@127.0.0.1:5432/havenmail
HAVENMAIL_DB_PASSWORD=${db_password}

HAVENMAIL_API_BIND=127.0.0.1:8080
HAVENMAIL_JWT_SIGNING_KEY=${jwt_key}
HAVENMAIL_SECRETS_KEY=${secrets_key}
EOF
  chmod 0640 "$HAVENMAIL_ENV_FILE"
  chown root:"$HAVENMAIL_SYSTEM_USER" "$HAVENMAIL_ENV_FILE"
}

havenmail_configure_postgres() {
  local db_password
  db_password="$(havenmail_env_get HAVENMAIL_DB_PASSWORD)"
  havenmail_log "Richte PostgreSQL-Rolle und Datenbank ein (idempotent)…"
  sudo -u postgres psql -v ON_ERROR_STOP=1 --quiet <<SQL
DO \$\$
BEGIN
  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'havenmail') THEN
    CREATE ROLE havenmail LOGIN PASSWORD '${db_password}';
  ELSE
    ALTER ROLE havenmail WITH PASSWORD '${db_password}';
  END IF;
END
\$\$;
SQL
  sudo -u postgres psql -v ON_ERROR_STOP=1 --quiet -tc \
    "SELECT 1 FROM pg_database WHERE datname = 'havenmail'" | grep -q 1 || \
    sudo -u postgres createdb --owner=havenmail havenmail
}

# Rendert die Postfix-/Dovecot-/Rspamd-Templates (config/*.tera) über die
# havenmail-cli und kopiert sie an ihre echten Systempfade. Bewusst über die
# bereits getestete Rust-Rendering-Logik (M1/config_render), nicht per
# sed/envsubst in Bash — vermeidet zwei parallele Template-Implementierungen.
havenmail_deploy_mail_configs() {
  local db_password mail_hostname render_dir="${HAVENMAIL_STATE_DIR}/rendered-config"
  db_password="$(havenmail_env_get HAVENMAIL_DB_PASSWORD)"
  mail_hostname="$(havenmail_env_get HAVENMAIL_HOSTNAME)"

  havenmail_log "Rendere Postfix-/Dovecot-/Rspamd-Konfiguration…"
  rm -rf "$render_dir"
  "${HAVENMAIL_REPO_DIR}/backend/target/release/havenmail-cli" render-configs \
    --config-dir "${HAVENMAIL_REPO_DIR}/config" \
    --out-dir "$render_dir" \
    --mail-hostname "$mail_hostname" \
    --db-password "$db_password" \
    --tls-cert-path "/etc/letsencrypt/live/${mail_hostname}/fullchain.pem" \
    --tls-key-path "/etc/letsencrypt/live/${mail_hostname}/privkey.pem"

  havenmail_log "Installiere gerenderte Konfiguration an Systempfade…"
  install -d -m 0755 /etc/postfix/havenmail
  install -m 0644 "${render_dir}/postfix/main.cf" /etc/postfix/main.cf
  cat "${render_dir}/postfix/master.cf.append" >> /etc/postfix/master.cf
  install -m 0640 -o root -g postfix "${render_dir}/postfix/pgsql-virtual-domains.cf" /etc/postfix/havenmail/
  install -m 0640 -o root -g postfix "${render_dir}/postfix/pgsql-virtual-mailboxes.cf" /etc/postfix/havenmail/
  install -m 0640 -o root -g postfix "${render_dir}/postfix/pgsql-virtual-aliases.cf" /etc/postfix/havenmail/

  install -d -m 0755 /etc/dovecot/conf.d
  install -m 0644 "${render_dir}/dovecot/10-mail.conf" /etc/dovecot/conf.d/10-mail.conf
  install -m 0644 "${render_dir}/dovecot/10-master.conf" /etc/dovecot/conf.d/10-master.conf
  install -m 0644 "${render_dir}/dovecot/10-ssl.conf" /etc/dovecot/conf.d/10-ssl.conf
  install -m 0640 -o root -g dovecot "${render_dir}/dovecot/dovecot-sql.conf.ext" /etc/dovecot/dovecot-sql.conf.ext

  install -d -m 0755 /etc/rspamd/local.d
  install -m 0644 "${render_dir}/rspamd/local.d/antivirus.conf" /etc/rspamd/local.d/antivirus.conf
  install -m 0644 "${render_dir}/rspamd/local.d/dkim_signing.conf" /etc/rspamd/local.d/dkim_signing.conf
  install -m 0644 "${render_dir}/rspamd/local.d/dmarc.conf" /etc/rspamd/local.d/dmarc.conf
  install -m 0644 "${render_dir}/rspamd/local.d/ratelimit.conf" /etc/rspamd/local.d/ratelimit.conf

  install -d -m 0755 /etc/fail2ban/filter.d
  install -m 0644 "${render_dir}/fail2ban/havenmail-postfix.conf" /etc/fail2ban/filter.d/havenmail-postfix.conf
  install -m 0644 "${render_dir}/fail2ban/havenmail-dovecot.conf" /etc/fail2ban/filter.d/havenmail-dovecot.conf

  postfix check
  dovecot -n >/dev/null
}

# TLS über certbot im Standalone-Modus, nur wenn noch kein Zertifikat für
# den Mail-Hostnamen existiert. Standalone statt --nginx-Plugin, weil noch
# keine nginx-vhost-Konfiguration ausgerollt ist (TODO M5: nginx-Template
# für die Admin-UI/Reverse-Proxy fehlt noch in config/, siehe
# docs/architecture.md); Port 80 ist an dieser Stelle im Ablauf noch frei,
# da nginx erst danach in havenmail_start_services gestartet wird.
# Erneuerung übernimmt certbots eigener systemd-Timer aus dem Debian-Paket.
havenmail_provision_tls() {
  local hostname="$1" admin_email="$2"
  if [[ -d "/etc/letsencrypt/live/${hostname}" ]]; then
    havenmail_log "TLS-Zertifikat für ${hostname} bereits vorhanden — überspringe Ausstellung."
    return 0
  fi
  havenmail_log "Fordere TLS-Zertifikat für ${hostname} über Let's Encrypt an (Standalone, Port 80)…"
  certbot certonly --standalone --preferred-challenges http --non-interactive --agree-tos \
    -m "$admin_email" -d "$hostname"
}

# Legt Domain + ersten super_admin an, sofern noch keiner existiert
# (siehe havenmail_core::bootstrap — idempotent). Das generierte Passwort
# wird ausschließlich in HAVENMAIL_ETC_DIR (0640, root:havenmail) abgelegt,
# nie auf stdout/stderr geloggt.
havenmail_bootstrap_admin() {
  local domain="$1" admin_local_part="$2"
  local admin_password credentials_file="${HAVENMAIL_ETC_DIR}/initial-admin-credentials"

  if [[ -f "$credentials_file" ]]; then
    havenmail_log "Admin-Zugangsdaten existieren bereits unter ${credentials_file} — überspringe."
    return 0
  fi

  admin_password="$(havenmail_random_secret 24)"
  havenmail_log "Lege ersten Administrator an (${admin_local_part}@${domain})…"
  "${HAVENMAIL_REPO_DIR}/backend/target/release/havenmail-cli" bootstrap-admin \
    --database-url "$(havenmail_env_get DATABASE_URL)" \
    --domain "$domain" \
    --local-part "$admin_local_part" \
    --password "$admin_password" >/dev/null

  umask 077
  cat > "$credentials_file" <<EOF
# Einmalig beim Erstinstall erzeugt. Nach dem ersten Login löschen/rotieren.
E-Mail: ${admin_local_part}@${domain}
Passwort: ${admin_password}
EOF
  chmod 0640 "$credentials_file"
  chown root:"$HAVENMAIL_SYSTEM_USER" "$credentials_file"
  havenmail_log "Admin-Zugangsdaten gespeichert unter ${credentials_file} (nur root/${HAVENMAIL_SYSTEM_USER} lesbar)."
}

havenmail_install_systemd_units() {
  havenmail_log "Installiere systemd-Unit für die Control-Plane-API…"
  install -m 0644 "${HAVENMAIL_REPO_DIR}/config/systemd/havenmail-api.service" \
    /etc/systemd/system/havenmail-api.service
  systemctl daemon-reload
  systemctl enable --quiet havenmail-api.service
}

havenmail_start_services() {
  havenmail_log "Starte Havenmail-Dienste…"
  systemctl restart havenmail-api.service
  systemctl restart postfix dovecot rspamd clamav-daemon nginx

  sleep 2
  if ! systemctl is-active --quiet havenmail-api.service; then
    havenmail_err "havenmail-api.service ist nicht aktiv. Logs: journalctl -u havenmail-api -e"
    exit 1
  fi
}

havenmail_verify_health() {
  local api_bind
  api_bind="$(havenmail_env_get HAVENMAIL_API_BIND)"
  api_bind="${api_bind:-127.0.0.1:8080}"
  if ! curl -fsS "http://${api_bind}/healthz" >/dev/null; then
    havenmail_err "Control-Plane-API antwortet nicht auf /healthz."
    exit 1
  fi
  havenmail_log "Control-Plane-API ist erreichbar (/healthz OK)."
}
