#!/usr/bin/env bash
# Havenmail — gemeinsame Preflight-Prüfungen für install.sh/update.sh.
# STATUS (M0): Grundfunktionen implementiert, wird in M5 erweitert
# (Portverfügbarkeit, RAM/Disk-Schwellenwerte final festlegen).
set -euo pipefail

havenmail_require_root() {
  if [[ "${EUID:-$(id -u)}" -ne 0 ]]; then
    echo "Fehler: Dieses Skript muss als root (bzw. mit sudo) ausgeführt werden." >&2
    exit 1
  fi
}

havenmail_require_debian() {
  if [[ ! -r /etc/os-release ]]; then
    echo "Fehler: /etc/os-release nicht gefunden — kein unterstütztes System." >&2
    exit 1
  fi
  # shellcheck disable=SC1091
  . /etc/os-release
  if [[ "${ID:-}" != "debian" ]]; then
    echo "Fehler: Nur Debian 12 (bookworm) und 13 (trixie) werden unterstützt (erkannt: ${ID:-unbekannt})." >&2
    exit 1
  fi
  case "${VERSION_ID:-}" in
    12|13) ;;
    *)
      echo "Fehler: Debian-Version ${VERSION_ID:-unbekannt} wird nicht unterstützt (benötigt: 12 oder 13)." >&2
      exit 1
      ;;
  esac
}

havenmail_check_arch() {
  local arch
  arch="$(uname -m)"
  case "$arch" in
    x86_64|aarch64) ;;
    *)
      echo "Fehler: Architektur ${arch} wird nicht unterstützt (benötigt: x86_64 oder aarch64)." >&2
      exit 1
      ;;
  esac
}

havenmail_check_min_ram_mb() {
  local required_mb="${1:-2048}"
  local available_mb
  available_mb=$(awk '/MemTotal/ {printf "%d", $2/1024}' /proc/meminfo)
  if (( available_mb < required_mb )); then
    echo "Warnung: ${available_mb} MiB RAM erkannt, empfohlen sind mindestens ${required_mb} MiB." >&2
  fi
}

havenmail_check_min_disk_gb() {
  local required_gb="${1:-20}"
  local path="${2:-/}"
  local available_gb
  available_gb=$(df --output=avail -BG "$path" | tail -n1 | tr -dc '0-9')
  if (( available_gb < required_gb )); then
    echo "Warnung: ${available_gb} GiB frei unter ${path}, empfohlen sind mindestens ${required_gb} GiB." >&2
  fi
}

havenmail_check_ports_free() {
  local port
  for port in "$@"; do
    if ss -Htln "sport = :${port}" 2>/dev/null | grep -q ":${port}"; then
      echo "Warnung: Port ${port} ist bereits belegt." >&2
    fi
  done
}
