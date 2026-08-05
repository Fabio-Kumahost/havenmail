-- Gesendete/empfangene Mail seit dem letzten Snapshot (echtes Delta, wie
-- clamav_detected_since_last) — für den Dashboard-Chart "Gesendet vs.
-- Empfangen", siehe havenmail-cli snapshot-metrics und routes/system.rs.
alter table metrics_snapshots add column mail_sent_count integer;
alter table metrics_snapshots add column mail_received_count integer;
