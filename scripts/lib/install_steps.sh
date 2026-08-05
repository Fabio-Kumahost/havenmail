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
    fail2ban
    ufw
    git
    curl
    openssl
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
  # RUSTUP_HOME/CARGO_HOME sind bereits durch common.sh exportiert (dort
  # auch die Begründung, warum das dort und nicht nur hier passiert).
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- -y --profile minimal --default-toolchain stable
  ln -sf "${CARGO_HOME}/bin/cargo" /usr/local/bin/cargo
  ln -sf "${CARGO_HOME}/bin/rustc" /usr/local/bin/rustc
}

havenmail_install_node() {
  # Debians eigenes nodejs-Paket (Bookworm: v18) ist zu alt für den
  # Vite/Rolldown-basierten Frontend-Build (braucht Node >= 20 — in einem
  # Debian-12-Testcontainer schlug der Build mit v18 real fehl: "node:util
  # does not provide an export named 'styleText'"). Debians Paket bringt
  # zudem hunderte transitive node-*-Systempakete mit und ist entsprechend
  # langsam zu installieren. Deshalb NodeSource statt apt.
  if command -v node >/dev/null 2>&1 && \
     [[ "$(node -e 'console.log(process.versions.node.split(".")[0])')" -ge 20 ]]; then
    return 0
  fi
  havenmail_log "Installiere Node.js 22.x LTS (NodeSource, nur für den Frontend-Build benötigt)…"
  curl -fsSL https://deb.nodesource.com/setup_22.x | bash - >/dev/null
  DEBIAN_FRONTEND=noninteractive apt-get install -y -qq nodejs
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
  ( cd "${HAVENMAIL_REPO_DIR}/backend" && cargo build --release --quiet )
}

havenmail_build_frontend() {
  havenmail_log "Baue Web-Oberfläche (npm run build)…"
  # Leere VITE_HAVENMAIL_API_URL -> das Frontend spricht relative Pfade
  # (/api/v1/...) an, die nginx same-origin zur Control-Plane proxied
  # (siehe config/nginx/havenmail.conf.tera). Vermeidet CORS vollständig,
  # statt eine Access-Control-Allow-Origin-Konfiguration pflegen zu müssen.
  ( cd "${HAVENMAIL_REPO_DIR}/frontend" && npm ci --silent && \
    VITE_HAVENMAIL_API_URL="" npm run build --silent )
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
  # apt startet den postgresql-Dienst auf einem normalen System zwar
  # üblicherweise automatisch beim Paket-Postinst — das ist aber implizites
  # Verhalten, keine Garantie (in einem Container mit policy-rc.d-Sperre
  # real fehlgeschlagen: "connection to server on socket ... failed: No
  # such file or directory"). enable --now ist idempotent und harmlos,
  # falls der Dienst schon läuft.
  systemctl enable --quiet --now postgresql 2>/dev/null || true
  db_password="$(havenmail_env_get HAVENMAIL_DB_PASSWORD)"
  havenmail_log "Richte PostgreSQL-Rolle und Datenbank ein (idempotent)…"
  # runuser statt sudo -u: sudo ist auf einem frischen Minimal-Debian nicht
  # zwingend vorinstalliert (in einem Debian-12-Testcontainer schlug genau
  # das fehl: "sudo: command not found"); runuser gehört zu util-linux
  # (essential-Paket, immer vorhanden) und install.sh läuft ohnehin bereits
  # als root, braucht also keinen Privilegien-Aufstieg über sudo.
  runuser -u postgres -- psql -v ON_ERROR_STOP=1 --quiet <<SQL
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
  runuser -u postgres -- psql -v ON_ERROR_STOP=1 --quiet -tc \
    "SELECT 1 FROM pg_database WHERE datname = 'havenmail'" | grep -q 1 || \
    runuser -u postgres -- createdb --owner=havenmail havenmail
}

# Zentraler Render-Pfad für alle Templates (Postfix/Dovecot/Rspamd/
# Fail2ban/nginx), einmal aufgerufen bevor die einzelnen deploy_*-Funktionen
# die für sie relevanten Dateien an Systempfade kopieren. Bewusst über die
# bereits getestete Rust-Rendering-Logik (M1/config_render), nicht per
# sed/envsubst in Bash — vermeidet zwei parallele Template-Implementierungen.
# Bezieht den TLS-Zertifikatspfad rein textuell (die Datei muss beim
# Rendern selbst noch nicht existieren, nur beim späteren nginx-Start mit
# dem vollen HTTPS-Vhost, siehe havenmail_deploy_nginx_full).
havenmail_render_configs() {
  local db_password mail_hostname
  db_password="$(havenmail_env_get HAVENMAIL_DB_PASSWORD)"
  mail_hostname="$(havenmail_env_get HAVENMAIL_HOSTNAME)"
  HAVENMAIL_RENDER_DIR="${HAVENMAIL_STATE_DIR}/rendered-config"

  havenmail_log "Rendere Konfigurationstemplates (Postfix/Dovecot/Rspamd/Fail2ban/nginx)…"
  rm -rf "$HAVENMAIL_RENDER_DIR"
  "${HAVENMAIL_REPO_DIR}/backend/target/release/havenmail-cli" render-configs \
    --config-dir "${HAVENMAIL_REPO_DIR}/config" \
    --out-dir "$HAVENMAIL_RENDER_DIR" \
    --mail-hostname "$mail_hostname" \
    --db-password "$db_password" \
    --tls-cert-path "/etc/letsencrypt/live/${mail_hostname}/fullchain.pem" \
    --tls-key-path "/etc/letsencrypt/live/${mail_hostname}/privkey.pem" \
    --frontend-dist-dir "${HAVENMAIL_REPO_DIR}/frontend/dist" \
    --api-bind "$(havenmail_env_get HAVENMAIL_API_BIND)"

  # Dovecot 2.4 replaced the legacy mail_location setting with the
  # mail_driver/mail_path pair. Debian 12 still ships Dovecot 2.3, so keep
  # the template portable and adapt only the rendered file on 2.4 hosts.
  local dovecot_version mail_conf compat_conf
  dovecot_version="$(dovecot --version 2>/dev/null | awk '{print $1}')"
  if [[ "$dovecot_version" == 2.4* ]]; then
    mail_conf="${HAVENMAIL_RENDER_DIR}/dovecot/10-mail.conf"
    compat_conf="$(mktemp)"
    awk '
      /^mail_location[[:space:]]*=[[:space:]]*maildir:/ {
        sub(/^[^:]*:[[:space:]]*/, "")
        # Dovecot 2.4 expandiert die alten Kurzform-Variablen (%d, %n, …)
        # in normalen Settings nicht mehr (nur noch %{user|domain} u.ä.
        # in passdb/userdb-Query-Substitution). Ohne diese Übersetzung
        # landet mail_path unexpandiert als Literal-Pfad
        # ".../%d/%n/" auf der Platte, Dovecot legt seinen
        # mailbox_list_index dort ab, findet die tatsächlichen
        # Nachrichten im echten Pfad nie und meldet dauerhaft 0
        # Nachrichten (real beobachtet: INBOX zeigte 0 Mails trotz
        # zugestellter Dateien im korrekten Maildir).
        gsub(/%d/, "%{user|domain}")
        gsub(/%n/, "%{user|username}")
        print "mail_driver = maildir"
        print "mail_path = " $0
        next
      }
      { print }
    ' "$mail_conf" > "$compat_conf"
    mv "$compat_conf" "$mail_conf"

    local master_conf
    master_conf="${HAVENMAIL_RENDER_DIR}/dovecot/10-master.conf"
    compat_conf="$(mktemp)"
    awk '
      /^[[:space:]]+address[[:space:]]*=[[:space:]]*127\.0\.0\.1[[:space:]]*$/ {
        print "    listen = 127.0.0.1"
        next
      }
      { print }
    ' "$master_conf" > "$compat_conf"
    mv "$compat_conf" "$master_conf"

    local ssl_conf
    ssl_conf="${HAVENMAIL_RENDER_DIR}/dovecot/10-ssl.conf"
    compat_conf="$(mktemp)"
    awk '
      /^ssl_cert[[:space:]]*=/ {
        sub(/^ssl_cert/, "ssl_server_cert_file")
        sub(/=[[:space:]]*</, "= ")
      }
      /^ssl_key[[:space:]]*=/ {
        sub(/^ssl_key/, "ssl_server_key_file")
        sub(/=[[:space:]]*</, "= ")
      }
      /^ssl_prefer_server_ciphers[[:space:]]*=/ {
        sub(/^ssl_prefer_server_ciphers/, "ssl_server_prefer_ciphers")
        sub(/[[:space:]]*=[[:space:]]*yes[[:space:]]*$/, " = server")
      }
      { print }
    ' "$ssl_conf" > "$compat_conf"
    mv "$compat_conf" "$ssl_conf"
  fi

  # Nichts bindet dovecot-sql.conf.ext bisher tatsächlich in Dovecots
  # Auth-Kette ein — 10-auth.conf ist Debians unveränderter Paket-Default
  # und lädt dort nur auth-system.conf.ext (PAM). Ohne diesen Schritt
  # scheitert *jeder* IMAP-/SMTP-Login mit "user unknown" gegen PAM,
  # weil virtuelle Havenmail-Postfächer keine Systembenutzer sind (real
  # bei einer Erstinstallation beobachtet: Dovecot startete sauber, aber
  # kein einziges Postfach konnte sich anmelden). Verbindungsdaten aus
  # der bereits gerenderten dovecot-sql.conf.ext ableiten statt sie hier
  # erneut zu templaten.
  local sql_ext="${HAVENMAIL_RENDER_DIR}/dovecot/dovecot-sql.conf.ext"
  local db_connect db_host db_port db_name db_user db_pass
  db_connect="$(grep '^connect = ' "$sql_ext" | sed 's/^connect = //')"
  db_host="$(grep -o 'host=[^ ]*' <<<"$db_connect" | cut -d= -f2)"
  db_port="$(grep -o 'port=[^ ]*' <<<"$db_connect" | cut -d= -f2)"
  db_name="$(grep -o 'dbname=[^ ]*' <<<"$db_connect" | cut -d= -f2)"
  db_user="$(grep -o 'user=[^ ]*' <<<"$db_connect" | cut -d= -f2)"
  db_pass="$(grep -o 'password=.*' <<<"$db_connect" | cut -d= -f2-)"

  local auth_sql_conf="${HAVENMAIL_RENDER_DIR}/dovecot/10-auth-sql.conf"
  if [[ "$dovecot_version" == 2.4* ]]; then
    # Dovecot 2.4 löste das alte "passdb sql { args = <file> }"-Muster
    # durch ein verschachteltes, benanntes Settings-Schema ab (per
    # doveconf -d ermittelt: passdb_sql_query, sql_driver, pgsql_* —
    # nicht dokumentiert unter dem alten dovecot-sql.conf.ext-Format).
    cat > "$auth_sql_conf" <<EOF
sql_driver = pgsql
pgsql pgsql {
  host = ${db_host}
  parameters {
    port = ${db_port}
    dbname = ${db_name}
    user = ${db_user}
    password = ${db_pass}
  }
}

passdb sql {
  passdb_sql_query = SELECT username AS user, password FROM dovecot_auth_users WHERE username = '%{user}' AND active
  passdb_default_password_scheme = ARGON2ID
}

userdb sql {
  userdb_sql_query = SELECT '/var/mail/havenmail/%{user|domain}/%{user|username}' AS home, 'maildir:/var/mail/havenmail/%{user|domain}/%{user|username}' AS mail, 5000 AS uid, 5000 AS gid, quota_bytes AS quota_rule FROM dovecot_auth_users WHERE username = '%{user}' AND active
  userdb_sql_iterate_query = SELECT username AS user FROM dovecot_auth_users WHERE active
}
EOF
  else
    # Dovecot 2.3 (Debian 12): klassische datei-referenzierte Syntax.
    # Unverändert seit vielen Dovecot-Versionen dokumentiert — im
    # Gegensatz zum 2.4-Zweig oben aber nicht auf einem echten
    # Debian-12-Host verifiziert, da diese Installation auf 2.4 lief.
    cat > "$auth_sql_conf" <<'EOF'
passdb {
  driver = sql
  args = /etc/dovecot/dovecot-sql.conf.ext
}
userdb {
  driver = sql
  args = /etc/dovecot/dovecot-sql.conf.ext
}
EOF
  fi
}

# Kopiert die gerenderte Postfix-/Dovecot-/Rspamd-/Fail2ban-Konfiguration an
# ihre echten Systempfade. Setzt voraus, dass havenmail_render_configs
# bereits gelaufen ist (HAVENMAIL_RENDER_DIR gesetzt).
havenmail_deploy_mail_configs() {
  local render_dir="$HAVENMAIL_RENDER_DIR"

  havenmail_log "Installiere gerenderte Mail-Engine-Konfiguration an Systempfade…"
  install -d -m 0755 /etc/postfix/havenmail
  install -m 0644 "${render_dir}/postfix/main.cf" /etc/postfix/main.cf
  cat "${render_dir}/postfix/master.cf.append" >> /etc/postfix/master.cf
  install -m 0640 -o root -g postfix "${render_dir}/postfix/pgsql-virtual-domains.cf" /etc/postfix/havenmail/
  install -m 0640 -o root -g postfix "${render_dir}/postfix/pgsql-virtual-mailboxes.cf" /etc/postfix/havenmail/
  install -m 0640 -o root -g postfix "${render_dir}/postfix/pgsql-virtual-aliases.cf" /etc/postfix/havenmail/
  install -m 0640 -o root -g postfix "${render_dir}/postfix/pgsql-sender-login-maps.cf" /etc/postfix/havenmail/

  install -d -m 0755 /etc/dovecot/conf.d
  install -m 0644 "${render_dir}/dovecot/10-mail.conf" /etc/dovecot/conf.d/10-mail.conf
  install -m 0644 "${render_dir}/dovecot/10-master.conf" /etc/dovecot/conf.d/10-master.conf
  install -m 0644 "${render_dir}/dovecot/10-ssl.conf" /etc/dovecot/conf.d/10-ssl.conf
  install -m 0640 -o root -g dovecot "${render_dir}/dovecot/dovecot-sql.conf.ext" /etc/dovecot/dovecot-sql.conf.ext
  install -m 0640 -o root -g dovecot "${render_dir}/dovecot/10-auth-sql.conf" /etc/dovecot/conf.d/10-auth-sql.conf
  # Debians dovecot-core-Paket aktiviert per Default PAM-Auth
  # (auth-system.conf.ext) — virtuelle Havenmail-Postfächer sind keine
  # Systembenutzer, PAM muss also weichen, sonst schlägt jeder Login fehl.
  sed -i 's/^!include auth-system\.conf\.ext/#!include auth-system.conf.ext/' \
    /etc/dovecot/conf.d/10-auth.conf

  # Gruppe havenmail statt root:root, Verzeichnis+Dateien gruppen-
  # beschreibbar: die Sicherheits-Einstellungsseiten im Admin-Panel
  # (routes/security_settings.rs) schreiben diese Dateien zur Laufzeit als
  # Systembenutzer "havenmail" neu (siehe ReadWritePaths in
  # havenmail-api.service). Weiterhin world-readable, da Rspamd selbst
  # unter einem eigenen Systemuser (_rspamd) läuft, nicht als havenmail.
  install -d -m 0775 -o root -g havenmail /etc/rspamd/local.d
  install -m 0664 -o root -g havenmail "${render_dir}/rspamd/local.d/actions.conf" /etc/rspamd/local.d/actions.conf
  install -m 0664 -o root -g havenmail "${render_dir}/rspamd/local.d/antivirus.conf" /etc/rspamd/local.d/antivirus.conf
  install -m 0644 "${render_dir}/rspamd/local.d/dkim_signing.conf" /etc/rspamd/local.d/dkim_signing.conf
  install -m 0664 -o root -g havenmail "${render_dir}/rspamd/local.d/dmarc.conf" /etc/rspamd/local.d/dmarc.conf
  install -m 0664 -o root -g havenmail "${render_dir}/rspamd/local.d/ratelimit.conf" /etc/rspamd/local.d/ratelimit.conf

  # jail.d, NICHT filter.d — das sind Jail-Definitionen (referenzieren
  # fail2bans mitgelieferte Filter), keine eigenen Filter-Regeln. Vorher
  # fälschlich nach filter.d installiert: fail2ban lud die Dateien nie
  # (kein Jail verweist auf einen Filter namens "havenmail-postfix"), real
  # beobachtet als nur ein aktiver Jail ("sshd") trotz enable --quiet
  # fail2ban.
  install -d -m 0755 /etc/fail2ban/jail.d
  install -m 0644 "${render_dir}/fail2ban/havenmail-postfix.conf" /etc/fail2ban/jail.d/havenmail-postfix.conf
  install -m 0644 "${render_dir}/fail2ban/havenmail-dovecot.conf" /etc/fail2ban/jail.d/havenmail-dovecot.conf

  postfix check
  dovecot -n >/dev/null
}

# Installiert den Übergangs-vhost (nur Port 80) und startet/reloaded
# nginx, damit certbot im Webroot-Modus die ACME-Challenge über ein
# tatsächlich laufendes nginx beantworten kann (kein --standalone, das mit
# einem später dauerhaft laufenden nginx um Port 80 konkurrieren würde).
havenmail_deploy_nginx_bootstrap() {
  install -d -m 0755 /var/www/havenmail-acme
  install -d -m 0755 /etc/nginx/sites-available
  install -m 0644 "${HAVENMAIL_RENDER_DIR}/nginx/havenmail-http.conf" \
    /etc/nginx/sites-available/havenmail.conf
  ln -sf /etc/nginx/sites-available/havenmail.conf /etc/nginx/sites-enabled/havenmail.conf
  rm -f /etc/nginx/sites-enabled/default
  nginx -t
  systemctl enable --quiet nginx
  systemctl restart nginx
}

# Ersetzt den Übergangs-vhost durch den vollen HTTPS-Vhost (Reverse-Proxy
# zur API, statische Auslieferung des Frontend-Builds). Muss erst NACH
# havenmail_provision_tls laufen, da die ssl_certificate-Direktiven auf
# existierende Dateien zeigen müssen, sonst schlägt `nginx -t` fehl.
havenmail_deploy_nginx_full() {
  install -m 0644 "${HAVENMAIL_RENDER_DIR}/nginx/havenmail.conf" \
    /etc/nginx/sites-available/havenmail.conf
  nginx -t
  systemctl reload nginx
}

# TLS über certbot im Webroot-Modus gegen das bereits laufende nginx
# (havenmail_deploy_nginx_bootstrap), nur wenn noch kein Zertifikat für den
# Mail-Hostnamen existiert. Erneuerung übernimmt certbots eigener
# systemd-Timer aus dem Debian-Paket — im Webroot-Modus ohne Dienst-Stopp,
# da kein Port-Konflikt mit nginx besteht.
havenmail_provision_tls() {
  local hostname="$1" admin_email="$2"

  havenmail_install_tls_expiry_hook

  if [[ -d "/etc/letsencrypt/live/${hostname}" ]]; then
    havenmail_log "TLS-Zertifikat für ${hostname} bereits vorhanden — überspringe Ausstellung."
    havenmail_write_tls_expiry_file "$hostname"
    return 0
  fi
  havenmail_log "Fordere TLS-Zertifikat für ${hostname} über Let's Encrypt an (Webroot)…"
  certbot certonly --webroot -w /var/www/havenmail-acme --non-interactive --agree-tos \
    -m "$admin_email" -d "$hostname"
  # certbots renewal-hooks/deploy/ laufen nur bei künftigen `certbot renew`,
  # nicht beim initialen `certonly` — deshalb hier einmalig manuell anstoßen.
  havenmail_write_tls_expiry_file "$hostname"
}

# Schreibt NUR das Ablaufdatum (nicht den privaten Schlüssel/Zertifikatsinhalt)
# an einen Pfad, den der unprivilegierte havenmail-Systembenutzer lesen darf
# — /etc/letsencrypt/live bleibt root:root 0700, wie certbot es vorgibt.
# Genutzt vom System-Status-Endpunkt (routes/system.rs) für die
# Zertifikatslaufzeit-Anzeige im Admin-Panel.
havenmail_write_tls_expiry_file() {
  local hostname="$1"
  local cert="/etc/letsencrypt/live/${hostname}/cert.pem"
  [[ -r "$cert" ]] || return 0
  local not_after
  not_after="$(openssl x509 -enddate -noout -in "$cert" | cut -d= -f2)"
  echo "$not_after" > "${HAVENMAIL_ETC_DIR}/tls-expiry"
  chmod 0644 "${HAVENMAIL_ETC_DIR}/tls-expiry"
}

# Installiert einen certbot-Deploy-Hook, der havenmail_write_tls_expiry_file
# bei jeder künftigen automatischen Erneuerung erneut ausführt (certbots
# systemd-Timer ruft `certbot renew`, das alle Skripte unter
# renewal-hooks/deploy/ startet).
havenmail_install_tls_expiry_hook() {
  install -d -m 0755 /etc/letsencrypt/renewal-hooks/deploy
  cat > /etc/letsencrypt/renewal-hooks/deploy/havenmail-tls-expiry.sh <<EOF
#!/usr/bin/env bash
# Von install.sh generiert (havenmail_install_tls_expiry_hook). RENEWED_LINEAGE
# wird von certbot beim Aufruf der renewal-hooks gesetzt.
set -euo pipefail
cert="\${RENEWED_LINEAGE}/cert.pem"
not_after="\$(openssl x509 -enddate -noout -in "\$cert" | cut -d= -f2)"
echo "\$not_after" > "${HAVENMAIL_ETC_DIR}/tls-expiry"
chmod 0644 "${HAVENMAIL_ETC_DIR}/tls-expiry"
EOF
  chmod 0755 /etc/letsencrypt/renewal-hooks/deploy/havenmail-tls-expiry.sh
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

  havenmail_log "Installiere Timer für periodische Dashboard-Metriken-Snapshots…"
  install -m 0644 "${HAVENMAIL_REPO_DIR}/config/systemd/havenmail-metrics-snapshot.service" \
    /etc/systemd/system/havenmail-metrics-snapshot.service
  install -m 0644 "${HAVENMAIL_REPO_DIR}/config/systemd/havenmail-metrics-snapshot.timer" \
    /etc/systemd/system/havenmail-metrics-snapshot.timer
  install -m 0644 "${HAVENMAIL_REPO_DIR}/config/systemd/havenmail-rspamd-reload.service" \
    /etc/systemd/system/havenmail-rspamd-reload.service
  install -m 0644 "${HAVENMAIL_REPO_DIR}/config/systemd/havenmail-rspamd-reload.path" \
    /etc/systemd/system/havenmail-rspamd-reload.path
  install -m 0644 "${HAVENMAIL_REPO_DIR}/config/systemd/havenmail-queue-delete.service" \
    /etc/systemd/system/havenmail-queue-delete.service
  install -m 0644 "${HAVENMAIL_REPO_DIR}/config/systemd/havenmail-queue-delete.path" \
    /etc/systemd/system/havenmail-queue-delete.path
  install -m 0644 "${HAVENMAIL_REPO_DIR}/config/systemd/havenmail-fail2ban-status.service" \
    /etc/systemd/system/havenmail-fail2ban-status.service
  install -m 0644 "${HAVENMAIL_REPO_DIR}/config/systemd/havenmail-fail2ban-status.timer" \
    /etc/systemd/system/havenmail-fail2ban-status.timer
  install -m 0644 "${HAVENMAIL_REPO_DIR}/config/systemd/havenmail-fail2ban-unban.service" \
    /etc/systemd/system/havenmail-fail2ban-unban.service
  install -m 0644 "${HAVENMAIL_REPO_DIR}/config/systemd/havenmail-fail2ban-unban.path" \
    /etc/systemd/system/havenmail-fail2ban-unban.path
  install -m 0644 "${HAVENMAIL_REPO_DIR}/config/systemd/havenmail-backup.service" \
    /etc/systemd/system/havenmail-backup.service
  install -m 0644 "${HAVENMAIL_REPO_DIR}/config/systemd/havenmail-backup.timer" \
    /etc/systemd/system/havenmail-backup.timer
  install -m 0644 "${HAVENMAIL_REPO_DIR}/config/systemd/havenmail-backup-trigger.path" \
    /etc/systemd/system/havenmail-backup-trigger.path
  install -m 0644 "${HAVENMAIL_REPO_DIR}/config/systemd/havenmail-notify-check.service" \
    /etc/systemd/system/havenmail-notify-check.service
  install -m 0644 "${HAVENMAIL_REPO_DIR}/config/systemd/havenmail-notify-check.timer" \
    /etc/systemd/system/havenmail-notify-check.timer

  systemctl daemon-reload
  systemctl enable --quiet havenmail-api.service
  systemctl enable --quiet --now havenmail-metrics-snapshot.timer
  systemctl enable --quiet --now havenmail-rspamd-reload.path
  systemctl enable --quiet --now havenmail-queue-delete.path
  systemctl enable --quiet --now havenmail-fail2ban-status.timer
  systemctl enable --quiet --now havenmail-fail2ban-unban.path
  systemctl enable --quiet --now havenmail-backup.timer
  systemctl enable --quiet --now havenmail-backup-trigger.path
  systemctl enable --quiet --now havenmail-notify-check.timer

}

# ClamAV startet nicht, solange keine Signaturdatenbank vorhanden ist
# (systemd-Bedingung `ConditionPathExistsGlob=/var/lib/clamav/daily.{cvd,cld}`
# auf clamav-daemon.service, real in einem Debian-12-Testcontainer
# beobachtet: der Dienst wurde stillschweigend übersprungen). `freshclam`
# einmalig blockierend laufen lassen, bevor der Daemon gestartet wird;
# clamav-freshclam.service danach aktivieren für künftige automatische
# Updates (läuft per Timer/Daemon-Modus weiter).
havenmail_provision_clamav() {
  if [[ ! -f /var/lib/clamav/daily.cvd && ! -f /var/lib/clamav/daily.cld ]]; then
    havenmail_log "Lade ClamAV-Signaturdatenbank (freshclam, einmalig — kann einige Minuten dauern)…"
    freshclam --quiet || havenmail_err "freshclam fehlgeschlagen — clamav-daemon startet ggf. nicht. Manuell prüfen: freshclam -v"
  fi
  systemctl enable --quiet clamav-freshclam.service 2>/dev/null || true
  systemctl restart clamav-freshclam.service 2>/dev/null || true
}

havenmail_start_services() {
  havenmail_provision_clamav

  havenmail_log "Starte Havenmail-Dienste…"
  systemctl restart havenmail-api.service
  systemctl enable --quiet fail2ban postfix dovecot rspamd clamav-daemon nginx 2>/dev/null || true
  systemctl restart postfix dovecot rspamd clamav-daemon nginx fail2ban

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
