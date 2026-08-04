-- Havenmail Kern-Datenmodell (siehe docs/architecture.md, Abschnitt Datenmodell).
-- Erzeugt Domains, Benutzer, Aliase, Verteiler, Weiterleitungen, Tokens,
-- Sessions, DKIM-Schlüssel, Audit-Log, Backup- und DNS-Check-Historie.

create extension if not exists pgcrypto;

create type havenmail_user_role as enum ('super_admin', 'domain_admin', 'user');

create table domains (
    id uuid primary key default gen_random_uuid(),
    name text not null unique,
    is_active boolean not null default true,
    catch_all_enabled boolean not null default false,
    catch_all_target text,
    dkim_selector text not null default 'havenmail',
    quota_bytes bigint,
    created_at timestamptz not null default now(),
    constraint catch_all_target_required_if_enabled
        check (not catch_all_enabled or catch_all_target is not null)
);

create table users (
    id uuid primary key default gen_random_uuid(),
    domain_id uuid not null references domains(id) on delete cascade,
    local_part text not null,
    password_hash text not null,
    role havenmail_user_role not null default 'user',
    quota_bytes bigint,
    is_active boolean not null default true,
    totp_secret_enc bytea,
    created_at timestamptz not null default now(),
    unique (domain_id, local_part)
);

create index users_domain_id_idx on users(domain_id);

create table aliases (
    id uuid primary key default gen_random_uuid(),
    domain_id uuid not null references domains(id) on delete cascade,
    source text not null,
    destinations text[] not null check (array_length(destinations, 1) > 0),
    is_active boolean not null default true,
    created_at timestamptz not null default now(),
    unique (domain_id, source)
);

create table distribution_lists (
    id uuid primary key default gen_random_uuid(),
    domain_id uuid not null references domains(id) on delete cascade,
    address text not null,
    members text[] not null default '{}',
    created_at timestamptz not null default now(),
    unique (domain_id, address)
);

create table forwards (
    id uuid primary key default gen_random_uuid(),
    user_id uuid not null references users(id) on delete cascade,
    target_address text not null,
    keep_copy boolean not null default true,
    -- Hash der Weiterleitungs-Zieladresse + bekannter Vorgänger-Ketten,
    -- von der Control-Plane vor Aktivierung zur Loop-Erkennung genutzt
    -- (siehe Bedrohungsanalyse in docs/architecture.md).
    loop_guard_hash text not null,
    is_active boolean not null default true,
    created_at timestamptz not null default now()
);

create table api_tokens (
    id uuid primary key default gen_random_uuid(),
    user_id uuid not null references users(id) on delete cascade,
    scopes text[] not null default '{}',
    token_hash text not null unique,
    expires_at timestamptz,
    revoked_at timestamptz,
    created_at timestamptz not null default now()
);

create index api_tokens_user_id_idx on api_tokens(user_id);

create table sessions (
    id uuid primary key default gen_random_uuid(),
    user_id uuid not null references users(id) on delete cascade,
    refresh_token_hash text not null unique,
    ip inet,
    user_agent text,
    created_at timestamptz not null default now(),
    revoked_at timestamptz
);

create index sessions_user_id_idx on sessions(user_id);

create table dkim_keys (
    id uuid primary key default gen_random_uuid(),
    domain_id uuid not null references domains(id) on delete cascade,
    selector text not null,
    private_key_enc bytea not null,
    public_key text not null,
    active boolean not null default true,
    created_at timestamptz not null default now(),
    unique (domain_id, selector)
);

create table audit_log (
    id uuid primary key default gen_random_uuid(),
    actor_id uuid references users(id) on delete set null,
    action text not null,
    target text not null,
    before jsonb,
    after jsonb,
    ip inet,
    created_at timestamptz not null default now(),
    prev_hash text,
    hash text not null unique
);

create index audit_log_created_at_idx on audit_log(created_at);

create table backup_runs (
    id uuid primary key default gen_random_uuid(),
    started_at timestamptz not null default now(),
    finished_at timestamptz,
    status text not null default 'running' check (status in ('running', 'success', 'failed')),
    size_bytes bigint,
    target text not null
);

create table dns_checks (
    id uuid primary key default gen_random_uuid(),
    domain_id uuid not null references domains(id) on delete cascade,
    record_type text not null,
    expected text not null,
    actual text,
    status text not null check (status in ('ok', 'missing', 'mismatch', 'unknown')),
    checked_at timestamptz not null default now()
);

create index dns_checks_domain_id_idx on dns_checks(domain_id);
