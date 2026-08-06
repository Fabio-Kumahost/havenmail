-- Zweiter Benachrichtigungskanal neben E-Mail (havenmail-cli notify-check,
-- siehe crates/core/src/notify.rs): ein Slack-kompatibler Webhook
-- (`{"text": "..."}`-POST, auch von Mattermost & vielen anderen Chat-Tools
-- akzeptiert). Lebt auf derselben security_settings-Singleton-Zeile wie
-- die Passwort-Richtlinie — kein eigenständiges Feature-Modul für eine
-- einzelne URL + Schalter nötig.
--
-- webhook_enabled getrennt von "URL gesetzt oder nicht": ein Admin kann
-- eine URL eintragen und über den Testversand-Button ausprobieren, ohne
-- dass sie sofort bei jedem echten Alarm live mitläuft.
alter table security_settings
    add column webhook_url text,
    add column webhook_enabled boolean not null default false;
