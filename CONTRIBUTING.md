# Beitrag zu Havenmail

Danke für dein Interesse an Havenmail. Da es sich um sicherheitskritische Infrastruktur (Mailserver) handelt, gelten erhöhte Sorgfaltsanforderungen an Beiträge.

## Grundregeln

- Keine Eigenimplementierung protokollkritischer oder kryptografischer Funktionen (SMTP, IMAP, JMAP, TLS, DKIM, Passwort-Hashing). Nutze etablierte, gepflegte Bibliotheken/Daemons — siehe [docs/architecture.md](docs/architecture.md).
- Jede Änderung an sicherheitsrelevantem Code (Auth, RBAC, Config-Rendering für Postfix/Dovecot/Rspamd, Installer) braucht ein Review durch mindestens eine weitere Person.
- Keine Secrets, Zugangsdaten oder privaten Schlüssel in Commits, Tests oder Beispieldateien.
- Commits sollen atomar und nachvollziehbar sein; Changelog-relevante Änderungen bitte in [CHANGELOG.md](CHANGELOG.md) unter „Unreleased" ergänzen.

## Entwicklungsumgebung

```bash
# Backend (Rust)
cd backend && cargo build && cargo test

# Frontend (TypeScript/React)
cd frontend && npm install && npm run build && npm test
```

## Pull Requests

1. Issue oder Diskussion vor größeren Änderungen, um Architektur-Konflikte früh zu klären.
2. Tests für neues Verhalten ergänzen; bestehende Tests dürfen nicht ohne Begründung entfernt werden.
3. `cargo fmt`, `cargo clippy -- -D warnings` sowie Frontend-Linter müssen fehlerfrei laufen.
4. CI muss vollständig grün sein, bevor ein Merge erfolgt.

## Verhaltenskodex

Sei respektvoll und konstruktiv. Beiträge, die auf Umgehung von Sicherheitsmechanismen oder das Einschleusen unsicherer Defaults abzielen, werden abgelehnt.
