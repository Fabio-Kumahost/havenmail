-- Rspamd-/ClamAV-Einstellungen, die über das Admin-Panel editierbar sind
-- (siehe routes/security_settings.rs). Singleton-Zeile (id fest auf 1).
-- Postgres ist die Quelle der Wahrheit; *.conf-Dateien unter /etc/rspamd
-- werden daraus gerendert (wie dkim_keys -> dkim_signing.conf), nicht
-- umgekehrt, damit ein erneuter Installer-/render-configs-Lauf editierte
-- Werte nicht verwirft.
create table security_settings (
    id smallint primary key default 1 check (id = 1),

    -- Rspamd: globale Score-Schwellen (rspamd/local.d/actions.conf).
    -- Defaults spiegeln rspamds eingebaute Werte, damit die erste
    -- Migration keinen stillen Verhaltenssprung auslöst.
    spam_greylist_score real not null default 4,
    spam_add_header_score real not null default 6,
    spam_reject_score real not null default 15,

    -- Rspamd-Module an/aus + Parameter
    dmarc_enabled boolean not null default true,
    ratelimit_enabled boolean not null default true,
    ratelimit_per_hour integer not null default 100,
    ratelimit_burst integer not null default 100,

    -- Antivirus (rspamd antivirus-Modul + ClamAV-Anbindung)
    antivirus_enabled boolean not null default true,
    antivirus_action text not null default 'reject'
        check (antivirus_action in ('reject', 'add_header', 'no_action')),
    antivirus_max_size_mb integer not null default 25,

    updated_at timestamptz not null default now(),
    updated_by uuid references users(id) on delete set null
);

insert into security_settings (id) values (1);
