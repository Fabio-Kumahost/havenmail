-- Abwesenheitsnotiz je Postfach (Sieve-"vacation"-Autoresponder, siehe
-- routes/vacation.rs). Dovecot Pigeonhole (dovecot-sieve/-managesieved)
-- war bisher installiert, aber ungenutzt — das Sieve-Plugin für die
-- LMTP-Zustellung war in Debians Stock-Config auskommentiert (siehe
-- config/dovecot/21-havenmail-lmtp.conf.tera).
--
-- Postgres bleibt Quelle der Wahrheit (wie bei security_settings/
-- dkim_keys): das eigentliche `.dovecot.sieve`-Skript im Mailbox-
-- Home-Verzeichnis ist nur eine daraus gerenderte Ableitung, kein
-- Nutzer bearbeitet es direkt (kein ManageSieve-Zugang aktiviert).
--
-- 1:1 zu users (PK = FK), da jedes Postfach höchstens eine aktive
-- Abwesenheitsnotiz hat — kein separates Postfach-übergreifendes Skript,
-- keine Mehrfachauswahl.
create table vacation_responders (
    user_id uuid primary key references users(id) on delete cascade,
    enabled boolean not null default false,
    subject text not null default 'Automatische Abwesenheitsnotiz',
    message text not null default '',
    -- NULL = kein Start-/Enddatum gesetzt, Notiz gilt zeitlich
    -- unbegrenzt (solange enabled = true).
    start_date date,
    end_date date check (start_date is null or end_date is null or end_date >= start_date),
    updated_at timestamptz not null default now()
);
