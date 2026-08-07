-- forwards.user_id hatte im Unterschied zu jeder anderen nutzerbezogenen
-- Tabelle (api_tokens, sessions) keinen Index, obwohl sowohl
-- list_forwards als auch der ON DELETE CASCADE beim Löschen eines Nutzers
-- gegen diese Spalte scannen (gefunden im Sicherheits-/Bug-Audit vom
-- 2026-08-07). Zusätzlich fehlte eine Uniqueness-Absicherung: create_forward
-- prüft zwar Selbst-Weiterleitung und Weiterleitungs-Schleifen, aber nicht,
-- ob exakt dieselbe (user_id, target_address)-Kombination schon existiert —
-- ein Nutzer/Admin konnte denselben Forward versehentlich doppelt anlegen.
create index forwards_user_id_idx on forwards (user_id);

create unique index forwards_user_target_unique_idx
    on forwards (user_id, target_address);
