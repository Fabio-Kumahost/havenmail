//! Rendert die beiden Rspamd-Lookup-Dateien (`selector_map`/`path_map`,
//! siehe config/rspamd/local.d/dkim_signing.conf.tera) aus der Liste der
//! aktuell AKTIVEN DKIM-Schlüssel — eine Zeile pro Domain, Format
//! "domain wert" (rspamds Standard-Key-Value-Map-Format). Reine
//! Zeichenketten-Erzeugung; das Schreiben/`configtest`/Reload übernimmt
//! der Aufrufer (siehe routes/dns.rs), analog zu
//! `config_render::render_security_settings`.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ActiveDkimKey {
    pub domain_name: String,
    pub selector: String,
}

/// "<dkim_dir>/<domain>/<selector>.pem" — derselbe Pfad wird sowohl beim
/// Erzeugen (Privatschlüssel schreiben) als auch beim Rendern der
/// `path_map` (nur für aktive Schlüssel) verwendet.
pub fn key_file_path(dkim_dir: &Path, domain_name: &str, selector: &str) -> PathBuf {
    dkim_dir.join(domain_name).join(format!("{selector}.pem"))
}

/// Eine Zeile je Domain: "<domain> <selector>".
pub fn render_selector_map(keys: &[ActiveDkimKey]) -> String {
    let mut lines: Vec<String> = keys
        .iter()
        .map(|k| format!("{} {}", k.domain_name, k.selector))
        .collect();
    lines.sort();
    lines.join("\n") + if lines.is_empty() { "" } else { "\n" }
}

/// Eine Zeile je Domain: "<domain> <absoluter-Pfad-zum-Privatschlüssel>".
pub fn render_path_map(dkim_dir: &Path, keys: &[ActiveDkimKey]) -> String {
    let mut lines: Vec<String> = keys
        .iter()
        .map(|k| {
            format!(
                "{} {}",
                k.domain_name,
                key_file_path(dkim_dir, &k.domain_name, &k.selector).display()
            )
        })
        .collect();
    lines.sort();
    lines.join("\n") + if lines.is_empty() { "" } else { "\n" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_file_path_joins_domain_and_selector() {
        let path = key_file_path(
            Path::new("/etc/havenmail/dkim"),
            "example.org",
            "dkim20260101",
        );
        assert_eq!(
            path,
            PathBuf::from("/etc/havenmail/dkim/example.org/dkim20260101.pem")
        );
    }

    #[test]
    fn renders_empty_maps_as_empty_string() {
        assert_eq!(render_selector_map(&[]), "");
        assert_eq!(render_path_map(Path::new("/etc/havenmail/dkim"), &[]), "");
    }

    #[test]
    fn renders_one_line_per_domain_sorted_by_domain_name() {
        let keys = vec![
            ActiveDkimKey {
                domain_name: "b.example".to_string(),
                selector: "dkim2".to_string(),
            },
            ActiveDkimKey {
                domain_name: "a.example".to_string(),
                selector: "dkim1".to_string(),
            },
        ];
        let selector_map = render_selector_map(&keys);
        assert_eq!(selector_map, "a.example dkim1\nb.example dkim2\n");

        let path_map = render_path_map(Path::new("/etc/havenmail/dkim"), &keys);
        assert_eq!(
            path_map,
            "a.example /etc/havenmail/dkim/a.example/dkim1.pem\n\
             b.example /etc/havenmail/dkim/b.example/dkim2.pem\n"
        );
    }
}
