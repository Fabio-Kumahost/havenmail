-- Domain-/postfachbezogenes Rate-Limiting: erlaubt es, das globale
-- Rspamd-Rate-Limit (security_settings.ratelimit_per_hour/_burst) für
-- einzelne Domains zu überschreiben — sowohl strenger (verdächtige/neue
-- Domain vorsichtshalber enger fassen) als auch lockerer (bekannter
-- Massenversender-Kunde, der legitim mehr Mail pro Stunde verschickt als
-- der globale Default erlaubt).
--
-- NULL = kein Override, Domain nutzt weiterhin den globalen Wert (siehe
-- routes/domains.rs, "leeres Feld im Frontend" entspricht NULL hier).
-- Liegt auf `domains` statt einer eigenen Tabelle, da es 1:1 pro Domain
-- ist und Domains bereits die natürliche Scope-Einheit für Rate-Limiting
-- sind (Rspamd rechnet ohnehin userseitig ab, nicht postfachweise genauer).
alter table domains
    add column ratelimit_per_hour_override integer
        check (ratelimit_per_hour_override is null or ratelimit_per_hour_override >= 1),
    add column ratelimit_burst_override integer
        check (ratelimit_burst_override is null or ratelimit_burst_override >= 1);
