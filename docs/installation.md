# Installation

> **Status:** Der Installer (`install.sh`) ist derzeit ein dokumentiertes Gerüst mit Preflight-Checks. Die eigentliche Paket-/Dienst-Installation folgt in Meilenstein M5 (siehe [architecture.md](architecture.md)). Führe `install.sh` bis dahin nicht auf einem produktiven Server aus.

## Voraussetzungen

- Debian 12 (bookworm) oder Debian 13 (trixie), amd64 oder arm64
- Root-Zugriff (oder `sudo`)
- Eigene Domain mit Zugriff auf die DNS-Verwaltung
- Öffentliche IPv4-Adresse mit korrektem Reverse-DNS (PTR-Eintrag) auf den Mail-Hostnamen
- Offene ausgehende Verbindung zu Let's-Encrypt (ACME) und Debian-Paketquellen
- Mindestens 2 GB RAM, 2 vCPU, 20 GB freier Speicher (vorläufige Empfehlung, wird in M5 anhand realer Lasttests validiert)

## Geplanter Ablauf des One-Liners

```bash
curl -fsSL https://raw.githubusercontent.com/USERNAME/havenmail/main/install.sh | sudo bash
```

## Empfohlene, sichere Variante

Skript vor Ausführung immer prüfen:

```bash
curl -fsSLo install.sh https://raw.githubusercontent.com/USERNAME/havenmail/main/install.sh
less install.sh
sudo bash install.sh
```

## Geplanter unbeaufsichtigter Modus

```bash
sudo HAVENMAIL_DOMAIN=example.org \
     HAVENMAIL_HOSTNAME=mail.example.org \
     HAVENMAIL_ADMIN_EMAIL=admin@example.org \
     HAVENMAIL_TIMEZONE=Europe/Berlin \
     bash install.sh --unattended
```

Diese Variablen und Flags sind das Zielbild aus der Architekturplanung und werden mit der Installer-Implementierung in M5 final festgelegt und dokumentiert.

## Nächste Schritte nach Installation (Zielbild)

1. DNS-Einträge gemäß Ausgabe des Installers setzen (siehe [dns-setup.md](dns-setup.md))
2. Admin-Oberfläche unter `https://<mail-hostname>/admin` mit den angezeigten Erstzugangsdaten aufrufen
3. Passwort ändern, 2FA aktivieren
4. Erste Domain und Benutzer anlegen
