-- Konfigurierbare Mindestlänge für Postfach-Passwörter. Bisher war "12"
-- an vier Stellen im Backend (create_user/update_user/change_own_password/
-- import_users) und zwei im Frontend (Account.tsx, DomainDetail.tsx) hart
-- codiert — ein super_admin, der z.B. 16 Zeichen als Mindestmaß durchsetzen
-- möchte (oder umgekehrt, für einen internen Testserver kürzer erlauben
-- will, aber nie unter 8), musste bisher den Quellcode ändern. Lebt in der
-- schon vorhandenen security_settings-Singleton-Zeile, da sie ohnehin die
-- Quelle der Wahrheit für alle live wirksamen Sicherheitseinstellungen ist.
--
-- Default bleibt 12, damit diese Migration keinen stillen
-- Verhaltenssprung auf bestehenden Installationen auslöst. Untergrenze 8
-- ist ein hartes Minimum in der DB (nicht nur im UI), damit ein
-- Fehlbedienung des Reglers nicht versehentlich effektiv unsichere
-- Passwörter erlaubt.
alter table security_settings
    add column min_password_length integer not null default 12
        check (min_password_length >= 8);
