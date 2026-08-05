-- Periodische Momentaufnahmen für Dashboard-Trendcharts (Rspamd-/ClamAV-/
-- Postfix-Metriken). Geschrieben von `havenmail-cli snapshot-metrics`
-- (systemd-Timer, siehe config/systemd/havenmail-metrics-snapshot.timer),
-- gelesen von GET /api/v1/system/metrics.
create table metrics_snapshots (
    id bigserial primary key,
    captured_at timestamptz not null default now(),

    -- Rspamd (kumulative Zähler aus dem Controller-`/stat`-Endpunkt, seit
    -- rspamd-Prozessstart -- die API bildet Deltas zwischen aufeinander-
    -- folgenden Zeilen für die Chart-Darstellung, siehe routes/system.rs)
    rspamd_scanned bigint,
    rspamd_spam_count bigint,
    rspamd_ham_count bigint,
    rspamd_action_reject bigint,
    rspamd_action_add_header bigint,
    rspamd_action_greylist bigint,
    rspamd_action_no_action bigint,

    -- ClamAV: seit dem letzten Snapshot neu geloggte "FOUND"-Treffer in
    -- /var/log/clamav/clamav.log (echtes Delta, kein kumulativer Zähler)
    clamav_detected_since_last integer,
    clamav_signature_age_hours integer,

    -- Postfix-Warteschlange (`postqueue -p`, Anzahl Einträge)
    mail_queue_size integer,

    -- Mail-Spool-Auslastung (`df /var/mail`)
    disk_used_percent real
);

create index metrics_snapshots_captured_at_idx on metrics_snapshots(captured_at);
