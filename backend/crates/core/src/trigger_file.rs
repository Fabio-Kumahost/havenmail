//! Atomares Schreiben von Trigger-Dateien für das Trigger-Datei +
//! root-eigene-systemd-Path-Unit-Muster (Rspamd-Reload, Mail-Warteschlangen-
//! Löschung, Fail2Ban-Entsperrung, Backup-Trigger — siehe die jeweiligen
//! `routes/*.rs`).
//!
//! Schreibt über eine temporäre Datei im selben Verzeichnis + `rename()`,
//! statt die Zieldatei direkt zu öffnen. `rename()` braucht nur Schreib-/
//! Ausführrecht auf das VERZEICHNIS, nicht auf eine eventuell bereits
//! bestehende Zieldatei. Ein direktes `std::fs::write()` auf den Zielpfad
//! würde dagegen dauerhaft mit "Permission denied" scheitern, falls die
//! Trigger-Datei aus irgendeinem Grund einmal mit anderem Besitzer/anderen
//! Rechten existiert (real aufgetreten: eine Trigger-Datei wurde einmalig
//! manuell als root angelegt, wonach der havenmail-Systembenutzer trotz
//! voller Rechte auf `/var/lib/havenmail` selbst nicht mehr hineinschreiben
//! konnte) — `rename()` umgeht diese Klasse von Problemen strukturell.

use std::path::Path;

pub fn write(path: &Path, content: &str) -> std::io::Result<()> {
    let dir = path
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Trigger-Pfad ohne Dateinamen",
        )
    })?;
    let tmp_path = dir.join(format!(
        ".{}.tmp-{}",
        file_name.to_string_lossy(),
        std::process::id()
    ));
    std::fs::write(&tmp_path, content)?;
    std::fs::rename(&tmp_path, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_content_reachable_at_target_path() {
        let dir =
            std::env::temp_dir().join(format!("havenmail-trigger-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("trigger-request");

        write(&target, "hello").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "hello");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn overwrites_a_preexisting_file_even_with_different_content() {
        let dir =
            std::env::temp_dir().join(format!("havenmail-trigger-test-b-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("trigger-request");
        std::fs::write(&target, "old").unwrap();

        write(&target, "new").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn succeeds_even_if_the_existing_target_file_is_unwritable() {
        // Genau das real aufgetretene Szenario: die Zieldatei existiert
        // bereits mit Rechten, die ein direktes std::fs::write() auf sie
        // scheitern lassen würden (hier: 0o000 statt "root-owned", da der
        // Testprozess selbst nicht root ist - der Effekt ist derselbe,
        // "kein Schreibrecht auf die bestehende Datei"). rename() über
        // eine temporäre Datei im selben, für uns beschreibbaren
        // Verzeichnis ist davon unbeeindruckt.
        use std::os::unix::fs::PermissionsExt;

        let dir =
            std::env::temp_dir().join(format!("havenmail-trigger-test-c-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("trigger-request");
        std::fs::write(&target, "old").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o000)).unwrap();

        write(&target, "new").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");

        std::fs::remove_dir_all(&dir).ok();
    }
}
