-- Zustand der periodischen Benachrichtigungs-Checks (`havenmail-cli
-- notify-check`, siehe config/systemd/havenmail-notify-check.timer).
-- Eine Zeile pro Check (z. B. "tls_expiry", "disk_usage",
-- "service:postfix", "backup") — verhindert, dass bei einem andauernden
-- Problem bei jedem 5-Minuten-Lauf erneut eine E-Mail rausgeht (nur beim
-- Zustandswechsel ok->problem sofort, danach höchstens einmal alle 24h als
-- Erinnerung, plus eine "behoben"-Mail beim Zustandswechsel problem->ok).
create table notification_state (
    check_key text primary key,
    status text not null check (status in ('ok', 'problem')),
    message text,
    first_seen_at timestamptz not null default now(),
    last_notified_at timestamptz,
    updated_at timestamptz not null default now()
);
