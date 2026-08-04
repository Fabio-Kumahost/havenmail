//! Havenmail CLI.
//!
//! STATUS (M0): Skeleton mit `status`-Subkommando. Domain-/Benutzer-/Alias-
//! Verwaltung folgt in M2 (siehe docs/architecture.md im Repo-Root).

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "havenmail-cli", version, about = "Havenmail Administrations-CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Zeigt den (aktuell rudimentären) Status der CLI selbst an.
    Status,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Status => {
            println!("Havenmail CLI — Skeleton (M0). Domain-/Benutzerverwaltung folgt in M2.");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_parses_status_subcommand() {
        let cli = Cli::parse_from(["havenmail-cli", "status"]);
        assert!(matches!(cli.command, Command::Status));
    }
}
