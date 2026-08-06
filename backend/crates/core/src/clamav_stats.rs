//! Liest ClamAV-Kennzahlen aus dem Dateisystem statt über eine clamd-RPC —
//! `clamd` selbst liefert per `STATS`-Kommando nur Pool-/Speicherzahlen,
//! keine kumulativen Fund-Zähler. Echte Treffer stehen ausschließlich im
//! Log (`FOUND`-Zeilen, `LogClean false` per Debian-Default) oder liefen
//! bereits über Rspamds `antivirus`-Modul (Milter-Pfad, siehe
//! config/rspamd/local.d/antivirus.conf.tera) — dieses Modul selbst führt
//! aber keine eigene Statistik, daher hier direkt am Log gemessen.

use chrono::{DateTime, Utc};
use std::path::Path;

/// Zählt "FOUND"-Zeilen in `log_path`, deren Zeitstempel nach `since`
/// liegt. Bestes-Bemühen-Parsing: eine nicht lesbare/parsebare Datei
/// liefert `0` statt eines Fehlers, ein Snapshot darf daran nicht
/// scheitern (siehe havenmail-cli snapshot-metrics, Option-toleranter
/// Sammel-Stil wie bei system.rs::query_unit_status).
pub fn detected_since(log_path: &Path, since: DateTime<Utc>) -> usize {
    let Ok(content) = std::fs::read_to_string(log_path) else {
        return 0;
    };

    content
        .lines()
        .filter(|line| line.contains("FOUND"))
        .filter(|line| line_after(line, since))
        .count()
}

/// ClamAV-Logzeilen beginnen mit `Mon DD HH:MM:SS YYYY -> ...` (Standard-
/// `LogTime`-Format). Kann der Zeitstempel nicht geparst werden, wird die
/// Zeile konservativ mitgezählt (lieber ein falsches Plus im Chart als
/// einen echten Fund zu verschlucken).
fn line_after(line: &str, since: DateTime<Utc>) -> bool {
    let Some(prefix) = line.get(0..24) else {
        return true;
    };
    match chrono::NaiveDateTime::parse_from_str(prefix.trim(), "%a %b %e %H:%M:%S %Y") {
        Ok(naive) => naive.and_utc() > since,
        Err(_) => true,
    }
}

/// Alter der aktuellen Signaturdatenbank in Stunden, ermittelt über die
/// mtime von `daily.cvd`/`daily.cld` (dieselbe Datei, die
/// `clamav-freshclam` aktualisiert) — kein Zugriff auf freshclams eigenes
/// Statusprotokoll nötig.
pub fn signature_age(clamav_lib_dir: &Path) -> Option<i64> {
    let candidates = ["daily.cvd", "daily.cld"];
    let newest = candidates
        .iter()
        .filter_map(|name| {
            std::fs::metadata(clamav_lib_dir.join(name))
                .ok()?
                .modified()
                .ok()
        })
        .max()?;
    let age = std::time::SystemTime::now().duration_since(newest).ok()?;
    Some(age.as_secs() as i64 / 3600)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn counts_found_lines_after_cutoff() {
        let file = tempfile_with_content(
            "Wed Aug  5 18:00:00 2026 -> /var/mail/.../a: Eicar-Test-Signature FOUND\n\
             Wed Aug  5 19:00:00 2026 -> /var/mail/.../b: OK\n\
             Wed Aug  5 20:30:00 2026 -> /var/mail/.../c: Win.Test.Malware FOUND\n",
        );
        let since = chrono::Utc.with_ymd_and_hms(2026, 8, 5, 19, 0, 0).unwrap();
        assert_eq!(detected_since(file.path(), since), 1);
    }

    #[test]
    fn missing_file_yields_zero_not_error() {
        assert_eq!(
            detected_since(Path::new("/nonexistent/clamav.log"), Utc::now()),
            0
        );
    }

    fn tempfile_with_content(content: &str) -> tempfile_shim::NamedTempFile {
        let file = tempfile_shim::NamedTempFile::new();
        std::fs::write(file.path(), content).expect("write temp file");
        file
    }

    /// Minimaler Ersatz für die `tempfile`-Crate (nicht in den
    /// Workspace-Dependencies vorhanden) — legt eine Datei unter
    /// `std::env::temp_dir()` mit zufälligem Namen an und räumt sie beim
    /// Drop wieder auf.
    mod tempfile_shim {
        use std::path::{Path, PathBuf};

        pub struct NamedTempFile {
            path: PathBuf,
        }

        impl NamedTempFile {
            pub fn new() -> Self {
                let path = std::env::temp_dir().join(format!(
                    "havenmail-clamav-test-{}.log",
                    uuid::Uuid::new_v4()
                ));
                Self { path }
            }

            pub fn path(&self) -> &Path {
                &self.path
            }
        }

        impl Drop for NamedTempFile {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.path);
            }
        }
    }
}
