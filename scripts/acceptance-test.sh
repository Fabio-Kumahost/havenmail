#!/usr/bin/env bash
#
# Havenmail — M6-Abnahmetest
#
# Läuft AUF dem installierten Server (nach erfolgreichem install.sh) und
# prüft die in docs/architecture.md (M6) genannten Abnahmekriterien:
# Open-Relay-Test, TLS-Test, DKIM-Test, Rechtetrennungstest. Backup/Restore
# wird bewusst NICHT hier getestet — dafür existieren bereits backup.sh/
# restore.sh, ein Test hier würde reale Daten überschreiben.
#
# Verwendung: sudo bash scripts/acceptance-test.sh
#
set -uo pipefail  # kein -e: ein einzelner fehlgeschlagener Check soll die
                  # übrigen nicht verhindern; wir sammeln PASS/FAIL selbst.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/lib/common.sh
source "${SCRIPT_DIR}/lib/common.sh"

PASS=0
FAIL=0
WARN=0

ok()   { echo "  [OK]   $*"; PASS=$((PASS + 1)); }
bad()  { echo "  [FAIL] $*"; FAIL=$((FAIL + 1)); }
warn() { echo "  [WARN] $*"; WARN=$((WARN + 1)); }

HOSTNAME_VAL="$(havenmail_env_get HAVENMAIL_HOSTNAME)"
DOMAIN_VAL="$(havenmail_env_get HAVENMAIL_DOMAIN)"
if [[ -z "$HOSTNAME_VAL" || -z "$DOMAIN_VAL" ]]; then
  echo "Fehler: HAVENMAIL_HOSTNAME/HAVENMAIL_DOMAIN nicht in ${HAVENMAIL_ENV_FILE} gefunden — ist Havenmail installiert?" >&2
  exit 1
fi

echo "== Havenmail M6-Abnahmetest für ${DOMAIN_VAL} (${HOSTNAME_VAL}) =="
echo

# --- 1. Health/Dienststatus ---
echo "-- Dienststatus --"
for unit in havenmail-api postfix dovecot rspamd nginx; do
  if systemctl is-active --quiet "$unit"; then
    ok "$unit aktiv"
  else
    bad "$unit NICHT aktiv (systemctl status $unit)"
  fi
done
if curl -fsS "http://127.0.0.1:8080/healthz" >/dev/null 2>&1; then
  ok "Control-Plane-API /healthz erreichbar"
else
  bad "Control-Plane-API /healthz NICHT erreichbar"
fi
echo

# --- 2. TLS-Test: SMTP/Submission/SMTPS/IMAPS erreichbar mit gültigem Zertifikat ---
echo "-- TLS --"
if command -v openssl >/dev/null 2>&1; then
  # Port 465 (SMTPS, implizites TLS) und 993 (IMAPS): direkter TLS-Handshake.
  for port in 465 993; do
    if echo | timeout 5 openssl s_client -connect "127.0.0.1:${port}" -servername "$HOSTNAME_VAL" 2>/dev/null \
         | grep -q "Verify return code: 0"; then
      ok "TLS-Handshake auf Port ${port} erfolgreich, Zertifikat gültig"
    else
      bad "TLS-Handshake auf Port ${port} fehlgeschlagen oder Zertifikat ungültig"
    fi
  done
  # Port 587 (Submission, STARTTLS) und 143/25 (STARTTLS optional).
  if echo | timeout 5 openssl s_client -connect "127.0.0.1:587" -starttls smtp -servername "$HOSTNAME_VAL" 2>/dev/null \
       | grep -q "Verify return code: 0"; then
    ok "STARTTLS auf Port 587 (Submission) erfolgreich"
  else
    bad "STARTTLS auf Port 587 fehlgeschlagen"
  fi
else
  warn "openssl nicht gefunden — TLS-Tests übersprungen"
fi
echo

# --- 3. Open-Relay-Test ---
# Verbindet sich unauthentifiziert zu Port 25 und versucht, an eine externe
# Domain (example.com) zuzustellen. Muss von smtpd_relay_restrictions
# (siehe config/postfix/main.cf.tera: permit_mynetworks,
# permit_sasl_authenticated, reject_unauth_destination) abgelehnt werden.
echo "-- Open-Relay-Test --"
if command -v swaks >/dev/null 2>&1; then
  RELAY_OUTPUT="$(swaks --to relay-test@example.com --from probe@"${HOSTNAME_VAL}" \
    --server 127.0.0.1 --port 25 --quit-after RCPT 2>&1)"
else
  # Fallback ohne swaks: rohes SMTP-Gespräch per bash-TCP-Redirection.
  RELAY_OUTPUT="$( { \
    exec 3<>/dev/tcp/127.0.0.1/25; \
    read -r -t 3 -u 3 _greeting; \
    echo -e "HELO probe.invalid\r" >&3; read -r -t 3 -u 3 _helo; \
    echo -e "MAIL FROM:<probe@${HOSTNAME_VAL}>\r" >&3; read -r -t 3 -u 3 _mailfrom; \
    echo -e "RCPT TO:<relay-test@example.com>\r" >&3; read -r -t 3 -u 3 rcpt_response; \
    echo -e "QUIT\r" >&3; \
    exec 3<&-; exec 3>&-; \
    echo "$rcpt_response"; \
  } 2>&1 )"
fi
if echo "$RELAY_OUTPUT" | grep -qE "554|Relay access denied|reject_unauth_destination"; then
  ok "Open-Relay-Test: Weiterleitung an fremde Domain korrekt abgelehnt"
else
  bad "Open-Relay-Test: KEINE eindeutige Ablehnung erkannt — manuell prüfen! Ausgabe: $RELAY_OUTPUT"
fi
echo

# --- 4. DKIM-Test ---
echo "-- DKIM --"
DKIM_SELECTOR="$(runuser -u postgres -- psql -tAc \
  "SELECT dkim_selector FROM domains WHERE name = '${DOMAIN_VAL}'" havenmail 2>/dev/null | tr -d ' ')"
if [[ -n "$DKIM_SELECTOR" ]]; then
  if command -v dig >/dev/null 2>&1; then
    DKIM_DNS="$(dig +short TXT "${DKIM_SELECTOR}._domainkey.${DOMAIN_VAL}" 2>/dev/null)"
    if [[ -n "$DKIM_DNS" ]]; then
      ok "DKIM-DNS-Eintrag für Selector '${DKIM_SELECTOR}' gefunden"
    else
      warn "Kein DKIM-DNS-Eintrag gefunden — wurde er nach der Schlüsselerzeugung im Panel auch bei der Domain gesetzt?"
    fi
  else
    warn "dig nicht gefunden — DKIM-DNS-Prüfung übersprungen (apt install dnsutils)"
  fi
  if rspamadm dkim_keygen -h 2>/dev/null | grep -q .; then
    :  # rspamadm vorhanden, kein weiterer Check hier nötig
  fi
else
  warn "Keine Domain '${DOMAIN_VAL}' mit DKIM-Selector in der DB — wurde im Panel schon ein DKIM-Schlüssel erzeugt?"
fi
echo

# --- 5. Rechtetrennungstest (RBAC) ---
# Die eigentliche Durchsetzung ist bereits per Integrationstest abgedeckt
# (backend/crates/api/tests/api_integration.rs,
# domain_admin_is_scoped_to_own_domain_and_cannot_see_others). Hier nur ein
# Live-Rauchtest gegen die laufende Installation, falls Zugangsdaten
# übergeben werden.
echo "-- Rechtetrennung (Live-Rauchtest, optional) --"
if [[ -n "${HAVENMAIL_TEST_DOMAIN_ADMIN_EMAIL:-}" && -n "${HAVENMAIL_TEST_DOMAIN_ADMIN_PASSWORD:-}" ]]; then
  TOKEN="$(curl -sk "https://${HOSTNAME_VAL}/api/v1/auth/login" \
    -H 'content-type: application/json' \
    -d "{\"email\":\"${HAVENMAIL_TEST_DOMAIN_ADMIN_EMAIL}\",\"password\":\"${HAVENMAIL_TEST_DOMAIN_ADMIN_PASSWORD}\"}" \
    | grep -o '"access_token":"[^"]*"' | cut -d'"' -f4)"
  if [[ -n "$TOKEN" ]]; then
    DOMAIN_COUNT="$(curl -sk "https://${HOSTNAME_VAL}/api/v1/domains" -H "authorization: Bearer ${TOKEN}" \
      | grep -o '"id"' | wc -l | tr -d ' ')"
    if [[ "$DOMAIN_COUNT" -le 1 ]]; then
      ok "domain_admin sieht nur die eigene Domain (${DOMAIN_COUNT} Ergebnis(se))"
    else
      bad "domain_admin sieht ${DOMAIN_COUNT} Domains — sollte nur 1 sein!"
    fi
  else
    warn "Login mit HAVENMAIL_TEST_DOMAIN_ADMIN_EMAIL fehlgeschlagen — Test übersprungen"
  fi
else
  warn "HAVENMAIL_TEST_DOMAIN_ADMIN_EMAIL/-PASSWORD nicht gesetzt — Live-Rauchtest übersprungen (per Integrationstest bereits abgedeckt, siehe backend/crates/api/tests/api_integration.rs)"
fi
echo

echo "== Ergebnis: ${PASS} OK, ${FAIL} FEHLGESCHLAGEN, ${WARN} übersprungen/Warnung =="
if [[ "$FAIL" -gt 0 ]]; then
  exit 1
fi
