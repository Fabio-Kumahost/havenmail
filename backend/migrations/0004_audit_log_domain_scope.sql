-- Domain-Zuordnung für Audit-Einträge, damit domain_admin (Action::ViewAuditLog,
-- siehe havenmail_core::rbac) nur Einträge der eigenen Domain sehen kann.
-- `target` allein (z.B. eine Benutzer-UUID) reicht dafür nicht aus, ohne bei
-- jeder Abfrage zurück auf users/domains zu joinen und dabei bereits
-- gelöschte Ressourcen zu verlieren.
alter table audit_log add column domain_id uuid references domains(id) on delete set null;
create index audit_log_domain_id_idx on audit_log(domain_id);
