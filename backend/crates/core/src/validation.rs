//! Zeichen-Whitelists für Domain-Namen und Postfach-Lokalteile.
//!
//! Vor diesem Modul wurden beide Werte nur auf "nicht leer" (lokaler Teil)
//! bzw. "nicht leer + enthält einen Punkt" (Domain) geprüft — beide landen
//! aber ungeprüft in Dateisystempfaden (DKIM-Schlüssel unter
//! `/etc/havenmail/dkim/<domain>/`, Dovecots `%d`/`%n`-Mailbox-Pfad-
//! Expansion) und in generierten Rspamd-Konfigurationsdateien
//! (Lua-String-Literale in `ratelimit.conf.tera`). Ein `local_part` wie
//! `../../../../tmp/evil` oder ein Domain-Name mit eingebettetem
//! Zeilenumbruch/Anführungszeichen konnte so Pfad-Traversal bzw.
//! Config-Injection ermöglichen (gefunden im Sicherheits-/Bug-Audit vom
//! 2026-08-07). Handgeschriebene Zeichenprüfung statt einer neuen
//! `regex`-Abhängigkeit — analog zu `fail2ban::is_valid_jail` und
//! `mail_queue::is_valid_queue_target` in diesem Crate.

/// Prüft `name` gegen RFC 1035/1123-Hostname-Regeln: mindestens zwei
/// durch `.` getrennte Labels, je Label 1–63 Zeichen aus `a-z0-9-`, weder
/// am Anfang noch am Ende ein Bindestrich, Gesamtlänge höchstens 253
/// Zeichen, nur ASCII (internationalisierte Domains müssen als Punycode,
/// `xn--…`, übergeben werden — das ist bereits reines ASCII und besteht
/// diese Prüfung unverändert).
pub fn is_valid_domain_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 253 || !name.is_ascii() {
        return false;
    }
    let labels: Vec<&str> = name.split('.').collect();
    if labels.len() < 2 {
        return false;
    }
    labels.iter().all(|label| is_valid_hostname_label(label))
}

fn is_valid_hostname_label(label: &str) -> bool {
    if label.is_empty() || label.len() > 63 {
        return false;
    }
    let bytes = label.as_bytes();
    let starts_ok = bytes[0].is_ascii_alphanumeric();
    let ends_ok = bytes[bytes.len() - 1].is_ascii_alphanumeric();
    starts_ok && ends_ok && bytes.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'-')
}

/// Prüft den Lokalteil einer Mailbox-Adresse (der Teil vor dem `@`): 1–64
/// Zeichen (RFC 5321-Obergrenze), nur ASCII-Kleinbuchstaben/-Ziffern sowie
/// `. _ + -`, weder am Anfang noch am Ende ein Punkt, keine zwei
/// aufeinanderfolgenden Punkte. Insbesondere kein `/`, `\` oder `..` als
/// Teilstring — genau das, was Dovecots `%n`-Pfad-Expansion (siehe
/// `config/dovecot/10-mail.conf.tera`) außerhalb des vorgesehenen
/// Maildir-Bereichs auflösen könnte.
pub fn is_valid_mailbox_local_part(local_part: &str) -> bool {
    if local_part.is_empty() || local_part.len() > 64 {
        return false;
    }
    let bytes = local_part.as_bytes();
    if bytes[0] == b'.' || bytes[bytes.len() - 1] == b'.' {
        return false;
    }
    if local_part.contains("..") {
        return false;
    }
    bytes
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'+' | b'-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_ordinary_domains() {
        assert!(is_valid_domain_name("example.org"));
        assert!(is_valid_domain_name("mail.sub.example.org"));
        assert!(is_valid_domain_name("xn--bcher-kva.example"));
        assert!(is_valid_domain_name("a-b.c-d.example"));
    }

    #[test]
    fn rejects_malformed_domains() {
        assert!(!is_valid_domain_name(""));
        assert!(!is_valid_domain_name("nodothere"));
        assert!(!is_valid_domain_name("-leadinghyphen.example"));
        assert!(!is_valid_domain_name("trailinghyphen-.example"));
        assert!(!is_valid_domain_name("has space.example"));
        assert!(!is_valid_domain_name("quote'.example"));
        assert!(!is_valid_domain_name("new\nline.example"));
        assert!(!is_valid_domain_name(".example"));
        assert!(!is_valid_domain_name("example."));
        assert!(!is_valid_domain_name("ü.example")); // non-ASCII muss als Punycode kommen
        assert!(!is_valid_domain_name(&format!("{}.example", "a".repeat(64))));
        assert!(!is_valid_domain_name(&format!(
            "{}.example",
            "a.".repeat(130)
        )));
    }

    #[test]
    fn accepts_ordinary_local_parts() {
        assert!(is_valid_mailbox_local_part("alice"));
        assert!(is_valid_mailbox_local_part("first.last"));
        assert!(is_valid_mailbox_local_part("user+tag"));
        assert!(is_valid_mailbox_local_part("under_score-hyphen.42"));
    }

    #[test]
    fn rejects_path_traversal_and_unsafe_local_parts() {
        assert!(!is_valid_mailbox_local_part(""));
        assert!(!is_valid_mailbox_local_part("../../../../tmp/evil"));
        assert!(!is_valid_mailbox_local_part("a/b"));
        assert!(!is_valid_mailbox_local_part("a\\b"));
        assert!(!is_valid_mailbox_local_part(".leadingdot"));
        assert!(!is_valid_mailbox_local_part("trailingdot."));
        assert!(!is_valid_mailbox_local_part("double..dot"));
        assert!(!is_valid_mailbox_local_part("Uppercase"));
        assert!(!is_valid_mailbox_local_part("space here"));
        assert!(!is_valid_mailbox_local_part(&"a".repeat(65)));
    }
}
