# Installation

> **Status:** `install.sh` ist vollständig implementiert (M5), per Docker-Debian-12-Simulation und einer echten VM-Installation end-to-end verifiziert (siehe CHANGELOG.md für die dabei gefundenen und behobenen Bugs).

## Voraussetzungen

- Debian 12 (bookworm) oder Debian 13 (trixie), amd64 oder arm64
- Root-Zugriff (oder `sudo`)
- Eigene Domain mit Zugriff auf die DNS-Verwaltung
- Öffentliche IPv4-Adresse mit korrektem Reverse-DNS (PTR-Eintrag) auf den Mail-Hostnamen, DNS A/AAAA-Eintrag für den Mail-Hostnamen bereits gesetzt (wird für die Let's-Encrypt-Ausstellung während der Installation benötigt, siehe [dns-setup.md](dns-setup.md))
- Offene eingehende Verbindungen auf Port 80/443 (ACME-Challenge, Admin-UI) und 25/587/465/143/993 (Mailverkehr)
- Offene ausgehende Verbindung zu Let's-Encrypt (ACME) und Debian-Paketquellen
- Mindestens 2 GB RAM, 2 vCPU, 20 GB freier Speicher (vorläufige Empfehlung, noch nicht anhand realer Lasttests validiert)

## One-Liner (Repo ist auf GitHub veröffentlicht)

```bash
curl -fsSL https://raw.githubusercontent.com/Fabio-Kumahost/havenmail/main/install.sh | sudo bash
```

Lädt zunächst nur `install.sh`; das Skript erkennt das Fehlen von `scripts/lib/`, klont daraufhin den vollständigen Quellcode nach `/opt/havenmail` (via `HAVENMAIL_SOURCE_REPO`, Standard [github.com/Fabio-Kumahost/havenmail](https://github.com/Fabio-Kumahost/havenmail)) und startet sich von dort neu.

## Alternative: manueller Transfer (z. B. eigener Fork, privater Checkout)

```bash
# per rsync (falls auf dem Server installiert):
rsync -a --exclude target --exclude node_modules --exclude .git \
  ~/havenmail/ root@mail.example.org:/opt/havenmail/

# oder git clone direkt auf dem Server:
ssh root@mail.example.org 'git clone https://github.com/Fabio-Kumahost/havenmail.git /opt/havenmail'

# Falls weder rsync noch git verfügbar sind (frische Minimal-Installation),
# alternativ tar über ssh — von macOS aus UNBEDINGT mit COPYFILE_DISABLE=1,
# sonst landen AppleDouble-Metadaten-Dateien (._*) im migrations-Verzeichnis
# und `cargo build` bricht mit "expected integer version prefix" ab (real
# sowohl in einem Testcontainer als auch bei einer echten Erstinstallation
# aufgetreten):
COPYFILE_DISABLE=1 tar -C ~/havenmail --exclude=backend/target \
  --exclude=frontend/node_modules --exclude=frontend/dist --exclude=.git \
  -cf - . | ssh root@mail.example.org 'mkdir -p /opt/havenmail && tar -C /opt/havenmail -xf -'

ssh root@mail.example.org
cd /opt/havenmail
sudo bash install.sh
```

`install.sh` erkennt anhand von `scripts/lib/common.sh` neben sich selbst, dass es bereits in einem vollständigen Checkout läuft, und installiert direkt von dort — kein Nachladen nötig.

## Empfohlene, sichere Variante

Skript vor Ausführung immer prüfen:

```bash
curl -fsSLo install.sh https://raw.githubusercontent.com/Fabio-Kumahost/havenmail/main/install.sh
less install.sh
sudo bash install.sh
```

## Unbeaufsichtigter Modus

```bash
sudo HAVENMAIL_DOMAIN=example.org \
     HAVENMAIL_HOSTNAME=mail.example.org \
     HAVENMAIL_ADMIN_EMAIL=admin@example.org \
     HAVENMAIL_TIMEZONE=Europe/Berlin \
     bash install.sh --unattended
```

Ohne `--unattended` fragt `install.sh` diese vier Werte interaktiv ab.

## Was der Installer tut

1. Preflight-Checks (root, Debian 12/13, Architektur, RAM/Disk, freie Ports)
2. Systembenutzer `havenmail` (uid 5000) und Verzeichnisse (`/etc/havenmail`, `/var/lib/havenmail`, `/var/mail/havenmail`, `/var/log/havenmail`)
3. apt-Pakete (PostgreSQL, Postfix, Dovecot, Rspamd, ClamAV, nginx, certbot, fail2ban, ufw, Build-Toolchain), Rust- und Node-Toolchain (nur falls nicht vorhanden)
4. PostgreSQL-Rolle/-Datenbank
5. Backend-/Frontend-Build (`cargo build --release`, `npm run build`)
6. Konfiguration rendern (Postfix/Dovecot/Rspamd/Fail2ban/nginx aus `config/*.tera`)
7. Übergangs-vhost (nur Port 80) installieren, nginx starten
8. TLS-Zertifikat via Let's Encrypt (certbot, Webroot-Modus)
9. Vollständige Mail-Engine-Konfiguration und den finalen HTTPS-vhost installieren
10. Firewall (ufw), systemd-Unit für die Control-Plane-API, alle Dienste starten
11. Health-Check (`/healthz`)
12. Ersten `super_admin` anlegen (Zugangsdaten unter `/etc/havenmail/initial-admin-credentials`, 0640)

Verifiziert per Docker-Debian-12-Simulation (systemd, echte Pakete/Build) und einer echten VM-Erstinstallation — siehe CHANGELOG.md für Details und dabei gefundene/behobene Bugs.

## Nächste Schritte nach Installation

1. DNS-Einträge gemäß Ausgabe des Installers setzen (siehe [dns-setup.md](dns-setup.md))
2. Admin-Oberfläche unter `https://<mail-hostname>/` mit den Zugangsdaten aus `/etc/havenmail/initial-admin-credentials` aufrufen
3. Passwort ändern, 2FA aktivieren
4. Erste Domain und Benutzer anlegen
5. `initial-admin-credentials` nach dem ersten Login löschen oder rotieren
6. Abnahmetest laufen lassen: `sudo bash scripts/acceptance-test.sh` (Open-Relay-, TLS-, DKIM-, Rechtetrennungs-Check; siehe docs/architecture.md, M6)
