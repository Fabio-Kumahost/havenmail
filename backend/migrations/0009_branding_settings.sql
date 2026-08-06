-- White-Label-Branding fürs Admin-Panel: Produktname, Logo-URL,
-- Akzentfarbe statt fest "Havenmail" — relevant, falls das Panel an
-- Kunden weitergegeben wird. Singleton-Tabelle (id=1), analog zu
-- security_settings (0005). Defaults spiegeln den heutigen Ist-Zustand
-- (Produktname "Havenmail", kein Logo, Standard-Akzentfarbe aus App.css)
-- — kein stiller optischer Sprung beim ersten Insert.
create table branding_settings (
    id integer primary key check (id = 1),
    product_name text not null default 'Havenmail',
    logo_url text,
    -- NULL = Standard-Akzentfarbe aus App.css (kein Override).
    accent_color text,
    updated_at timestamptz not null default now(),
    updated_by uuid references users(id) on delete set null
);

insert into branding_settings (id) values (1);
