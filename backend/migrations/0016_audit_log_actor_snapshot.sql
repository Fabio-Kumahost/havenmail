-- audit_log.actor_id ist eine live FK (on delete set null) — ihr Wert
-- fließt aber in audit_log.hash ein (siehe havenmail_core::audit). Löscht
-- ein super_admin einen Nutzer, mutiert der ON DELETE SET NULL-Cascade
-- jeden Audit-Log-Eintrag, den dieser Nutzer je verfasst hat (actor_id ->
-- NULL), ohne den gespeicherten Hash neu zu berechnen — jede spätere
-- Kettenverifikation (verify_chain) würde diese Einträge fälschlich als
-- "nachträglich manipuliert" melden (gefunden im Sicherheits-/Bug-Audit
-- vom 2026-08-07).
--
-- Fix: eine zweite, NICHT FK-referenzierte Spalte hält den Actor-UUID-Wert
-- exakt so fest, wie er beim Einfügen gehasht wurde — unveränderlich, da
-- kein ON DELETE CASCADE/SET NULL sie je anfassen kann. `actor_id` selbst
-- bleibt als bequeme FK für Joins/"ist dieser Nutzer noch aktiv"-Abfragen
-- erhalten, wird aber nicht mehr für die Hash-Verifikation gelesen.
alter table audit_log add column actor_id_snapshot uuid;
update audit_log set actor_id_snapshot = actor_id;
