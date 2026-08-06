//! Rendert das Sieve-Skript für die Abwesenheitsnotiz (siehe
//! `routes/vacation.rs`) aus Nutzereingaben (Betreff/Nachrichtentext,
//! optionaler Zeitraum). Reine Zeichenketten-Erzeugung, keine Datei-/
//! Prozess-Interaktion — das macht der Aufrufer (schreibt das Ergebnis,
//! kompiliert per `sievec`, siehe routes/vacation.rs::apply_to_mailbox).
//!
//! Betreff/Nachricht kommen vom Nutzer selbst (Selbstbedienung, siehe
//! `change_own_password`-Analogie in `routes/users.rs`) und müssen sicher
//! in Sieve-Syntax eingebettet werden, um Sieve-Injection zu verhindern
//! (ein `"`/`\` im Betreff dürfte sonst aus dem Quoted-String ausbrechen).

use chrono::NaiveDate;

/// Escaped einen Sieve-Quoted-String-Inhalt (`\` -> `\\`, `"` -> `\"`).
/// Zeilenumbrüche werden entfernt (Sieve-Quoted-Strings erlauben keine
/// rohen Newlines) statt sie zu escapen — der Betreff ist ohnehin
/// einzeilig gedacht.
fn escape_quoted(input: &str) -> String {
    input
        .chars()
        .filter(|c| *c != '\n' && *c != '\r')
        .flat_map(|c| match c {
            '\\' => vec!['\\', '\\'],
            '"' => vec!['\\', '"'],
            other => vec![other],
        })
        .collect()
}

/// Encodiert den Nachrichtentext als Sieve-"text:"-Literal (RFC 5228
/// §2.4.2), inklusive Dot-Stuffing: Zeilen, die mit einem Punkt beginnen,
/// bekommen einen zusätzlichen Punkt vorangestellt (analog zum
/// SMTP-DATA-Verfahren), sonst würde eine Zeile wie "." das Literal
/// vorzeitig beenden bzw. eine mit "." beginnende Zeile fehlinterpretiert.
fn encode_text_literal(message: &str) -> String {
    message
        .lines()
        .map(|line| {
            if line.starts_with('.') {
                format!(".{line}")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Baut die `currentdate`-Bedingung für einen optionalen Start-/End-Zeitraum.
/// `None`/`None` liefert `None` (keine Bedingung, Notiz gilt unbegrenzt).
fn date_condition(start: Option<NaiveDate>, end: Option<NaiveDate>) -> Option<String> {
    let fmt = |d: NaiveDate| d.format("%Y-%m-%d").to_string();
    match (start, end) {
        (None, None) => None,
        (Some(s), None) => Some(format!(
            "currentdate :value \"ge\" \"date\" \"{}\"",
            fmt(s)
        )),
        (None, Some(e)) => Some(format!(
            "currentdate :value \"le\" \"date\" \"{}\"",
            fmt(e)
        )),
        (Some(s), Some(e)) => Some(format!(
            "allof(currentdate :value \"ge\" \"date\" \"{}\", currentdate :value \"le\" \"date\" \"{}\")",
            fmt(s),
            fmt(e)
        )),
    }
}

/// Rendert das vollständige Sieve-Skript. `address` ist die volle
/// E-Mail-Adresse des Postfachs (lokal@domain) — als `:addresses`
/// explizit an die `vacation`-Aktion übergeben, weil Pigeonhole sonst bei
/// implizit zugestellter Mail (LMTP kennt nur die Envelope-Adresse, nicht
/// zwingend einen passenden To/Cc-Header) die Antwort verwirft
/// ("no known (envelope) recipient address found", live gegen den echten
/// Server verifiziert).
///
/// `:days 1` begrenzt Wiederholungsantworten an denselben Absender auf
/// höchstens einmal pro Tag — ein fester, bewusst nicht konfigurierbarer
/// Wert (verhindert Autoresponder-Ping-Pong-Loops mit anderen
/// Autorespondern, gängiger Sieve-Best-Practice-Default).
pub fn render_vacation_script(address: &str, subject: &str, message: &str) -> String {
    let subject_esc = escape_quoted(subject);
    let address_esc = escape_quoted(address);
    let text = encode_text_literal(message);
    format!(
        "require [\"vacation\"];\n\nvacation :days 1 :addresses \"{address_esc}\" :subject \"{subject_esc}\" text:\n{text}\n.\n;\n"
    )
}

/// Wie [`render_vacation_script`], aber mit optionalem Gültigkeitszeitraum
/// (`start_date`/`end_date`) — wickelt die `vacation`-Aktion in ein
/// `if currentdate ...`, damit sie außerhalb des Zeitraums gar nicht erst
/// ausgeführt wird (die Notiz bleibt technisch "enabled", greift aber nur
/// im konfigurierten Fenster).
pub fn render_vacation_script_with_range(
    address: &str,
    subject: &str,
    message: &str,
    start_date: Option<NaiveDate>,
    end_date: Option<NaiveDate>,
) -> String {
    let Some(condition) = date_condition(start_date, end_date) else {
        return render_vacation_script(address, subject, message);
    };

    let subject_esc = escape_quoted(subject);
    let address_esc = escape_quoted(address);
    let text = encode_text_literal(message);
    format!(
        "require [\"vacation\", \"date\", \"relational\"];\n\nif {condition} {{\n  vacation :days 1 :addresses \"{address_esc}\" :subject \"{subject_esc}\" text:\n{text}\n.\n;\n}}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_simple_vacation_without_date_range() {
        let out = render_vacation_script("root@example.org", "Abwesend", "Bin nicht da.");
        assert!(out.contains("require [\"vacation\"];"));
        assert!(out.contains(":addresses \"root@example.org\""));
        assert!(out.contains(":subject \"Abwesend\""));
        assert!(out.contains("Bin nicht da."));
        assert!(!out.contains("currentdate"));
    }

    #[test]
    fn escapes_quotes_and_backslashes_in_subject_to_prevent_sieve_injection() {
        let out = render_vacation_script(
            "root@example.org",
            "Weg\" :addresses \"admin@evil.example",
            "Text",
        );
        // Der eingebettete Anführungsstrich darf den Quoted-String nicht
        // verlassen — muss im Ergebnis escaped als \" auftauchen, nicht
        // als eigenständiges Syntaxzeichen.
        assert!(out.contains(r#":subject "Weg\" :addresses \"admin@evil.example""#));
    }

    #[test]
    fn escapes_backslash_before_quote_to_avoid_ambiguous_escaping() {
        let out = render_vacation_script("root@example.org", r#"back\slash"#, "Text");
        assert!(out.contains(r#":subject "back\\slash""#));
    }

    #[test]
    fn strips_newlines_from_subject() {
        let out = render_vacation_script("root@example.org", "Zeile1\nZeile2", "Text");
        assert!(out.contains(":subject \"Zeile1Zeile2\""));
    }

    #[test]
    fn dot_stuffs_message_lines_starting_with_a_dot() {
        let out = render_vacation_script("root@example.org", "S", ".hallo\nnormal\n..zwei");
        assert!(out.contains("\n..hallo\n"));
        assert!(out.contains("\nnormal\n"));
        assert!(out.contains("\n...zwei\n"));
    }

    #[test]
    fn renders_start_only_range_as_ge_condition() {
        let out = render_vacation_script_with_range(
            "root@example.org",
            "S",
            "M",
            Some(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()),
            None,
        );
        assert!(out.contains(r#"currentdate :value "ge" "date" "2026-01-01""#));
        assert!(!out.contains("allof"));
        assert!(out.contains("require [\"vacation\", \"date\", \"relational\"];"));
    }

    #[test]
    fn renders_end_only_range_as_le_condition() {
        let out = render_vacation_script_with_range(
            "root@example.org",
            "S",
            "M",
            None,
            Some(NaiveDate::from_ymd_opt(2026, 1, 31).unwrap()),
        );
        assert!(out.contains(r#"currentdate :value "le" "date" "2026-01-31""#));
    }

    #[test]
    fn renders_full_range_as_allof_condition() {
        let out = render_vacation_script_with_range(
            "root@example.org",
            "S",
            "M",
            Some(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()),
            Some(NaiveDate::from_ymd_opt(2026, 1, 31).unwrap()),
        );
        assert!(out.contains("allof(currentdate :value \"ge\" \"date\" \"2026-01-01\", currentdate :value \"le\" \"date\" \"2026-01-31\")"));
    }

    #[test]
    fn no_range_falls_back_to_simple_render() {
        let with_range = render_vacation_script_with_range("a@b.c", "S", "M", None, None);
        let simple = render_vacation_script("a@b.c", "S", "M");
        assert_eq!(with_range, simple);
    }
}
