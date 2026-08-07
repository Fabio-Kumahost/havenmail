-- Bislang wirkte sich die forwards-Tabelle NIE auf tatsächliche
-- Mailzustellung aus: create_forward (routes/forwards.rs) schreibt zwar
-- mit Loop-Schutz in die Tabelle und bestätigt 200 OK, aber
-- postfix_virtual_aliases (die View, die Postfix' virtual_alias_maps
-- tatsächlich per SQL-Lookup abfragt) berücksichtigte forwards nie — eine
-- eingerichtete Weiterleitung sah im Admin-Panel aktiv aus, hatte aber nie
-- irgendeine Wirkung auf echte Mail (gefunden im Sicherheits-/Bug-Audit
-- vom 2026-08-07).
--
-- Fix: forwards fließt jetzt als zwei zusätzliche union-all-Zweige in
-- postfix_virtual_aliases ein. Postfix' virtual_alias_maps ERSETZT bei
-- einem Treffer den ursprünglichen Empfänger vollständig (die normale
-- lokale Mailbox-Zustellung aus postfix_virtual_mailboxes greift dann
-- NICHT mehr) — deshalb zwei Zeilen für keep_copy=true (eigene
-- Mailbox-Adresse ALS EINE der Zieladressen zusätzlich zu
-- target_address, damit die lokale Kopie erhalten bleibt) und nur eine
-- Zeile für keep_copy=false (nur target_address, altes Verhalten:
-- ausschließlich weiterleiten).
create or replace view postfix_virtual_aliases as
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
where d.is_active and d.catch_all_enabled and d.catch_all_target is not null

union all

select
    u.local_part || '@' || d.name as source_address,
    f.target_address as destination
from forwards f
join users u on u.id = f.user_id
join domains d on d.id = u.domain_id
where f.is_active and u.is_active and d.is_active

union all

select
    u.local_part || '@' || d.name as source_address,
    u.local_part || '@' || d.name as destination
from forwards f
join users u on u.id = f.user_id
join domains d on d.id = u.domain_id
where f.is_active and f.keep_copy and u.is_active and d.is_active;
