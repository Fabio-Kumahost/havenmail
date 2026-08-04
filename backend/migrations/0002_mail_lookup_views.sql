-- Views, die von Postfix (virtual_mailbox/alias_maps) und Dovecot
-- (SASL-Passwortabfrage) per direktem SQL-Lookup abgefragt werden
-- (siehe config/postfix/*.cf und config/dovecot/dovecot-sql.conf.ext).
-- Es wird bewusst kein Zwischenlayer in der Control-Plane für diese
-- Lookups benötigt — Postfix/Dovecot fragen read-only direkt gegen
-- Postgres ab, was der etablierte, gut dokumentierte Integrationsweg
-- dieser Daemons ist und keinerlei eigene Protokoll-/Vermittlungslogik
-- erfordert.

-- Postfix: virtual_mailbox_domains (nur aktive Domains sind zustellbar)
create view postfix_virtual_domains as
select name
from domains
where is_active;

-- Postfix: virtual_mailbox_maps (lokale Postfächer -> Maildir-Pfad)
create view postfix_virtual_mailboxes as
select
    u.local_part || '@' || d.name as email,
    d.name || '/' || u.local_part || '/' as maildir
from users u
join domains d on d.id = u.domain_id
where u.is_active and d.is_active;

-- Postfix: virtual_alias_maps (Aliase + Verteiler + Catch-all)
create view postfix_virtual_aliases as
select
    a.source || '@' || d.name as source_address,
    unnest(a.destinations) as destination
from aliases a
join domains d on d.id = a.domain_id
where a.is_active and d.is_active

union all

select
    dl.address || '@' || d.name as source_address,
    unnest(dl.members) as destination
from distribution_lists dl
join domains d on d.id = dl.domain_id
where d.is_active and array_length(dl.members, 1) > 0

union all

select
    '@' || d.name as source_address,
    d.catch_all_target as destination
from domains d
where d.is_active and d.catch_all_enabled and d.catch_all_target is not null;

-- Dovecot: SASL-Passwortabfrage für Postfix-Submission und IMAP-Login.
-- `password` enthält den Argon2id-PHC-String; Dovecot verifiziert selbst
-- gegen das vom Client übermittelte Klartextpasswort (Dovecots eingebautes
-- ARGON2ID-Auth-Scheme, keine Eigenimplementierung).
create view dovecot_auth_users as
select
    u.local_part || '@' || d.name as username,
    u.password_hash as password,
    u.is_active as active,
    u.quota_bytes,
    d.name as domain
from users u
join domains d on d.id = u.domain_id;
