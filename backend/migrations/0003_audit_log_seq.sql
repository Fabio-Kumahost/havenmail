-- Deterministische Einfüge-Reihenfolge für die Audit-Log-Hash-Chain
-- (havenmail_core::audit): `created_at` allein kann bei schneller
-- Aufeinanderfolge auf denselben Mikrosekunden-Wert kollidieren, was die
-- Bestimmung des "letzten" Eintrags (und damit prev_hash) mehrdeutig
-- machen würde. `seq` ist strikt monoton in Einfüge-Reihenfolge.
alter table audit_log add column seq bigserial;
create index audit_log_seq_idx on audit_log(seq);
