# Fehlerbehebung

> **Status:** Dieses Dokument wird mit jedem Meilenstein um konkrete, getestete Fehlerbilder ergänzt. Aktuell (M0) gibt es noch keinen lauffähigen Serverbetrieb, daher keine produktiven Troubleshooting-Fälle.

## Diagnose-Grundgerüst (Zielbild)

```bash
havenmail-cli status       # Dienststatus, Health-Checks
havenmail-cli logs <dienst> # strukturierte Logs eines Dienstes
havenmail-cli diagnose      # gebündelter Diagnosebericht (DNS, TLS, Queues)
```

## Bekannte Einschränkungen im aktuellen Stand (M0)

- Kein Mailserver-Betrieb möglich (Postfix/Dovecot/Rspamd-Orchestrierung fehlt noch)
- Installer führt noch keine echte Installation durch
