//! Havenmail CLI.
//!
//! STATUS (M2): spricht die REST-Admin-API an (Login, Domain-/Benutzer-
//! verwaltung). Zugangstoken werden lokal unter
//! `~/.config/havenmail/credentials.json` abgelegt (0600). Base-URL über
//! `--api-url` oder `HAVENMAIL_API_URL` (Standard: http://127.0.0.1:8080).

use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "havenmail-cli",
    version,
    about = "Havenmail Administrations-CLI"
)]
struct Cli {
    /// Basis-URL der Control-Plane-API.
    #[arg(
        long,
        env = "HAVENMAIL_API_URL",
        default_value = "http://127.0.0.1:8080"
    )]
    api_url: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Bei der Control-Plane anmelden und Token lokal speichern.
    Login {
        #[arg(long)]
        email: String,
        #[arg(long)]
        password: String,
    },
    /// Domain anlegen.
    DomainCreate { name: String },
    /// Alle für den angemeldeten Account sichtbaren Domains auflisten.
    DomainList,
    /// Benutzer in einer Domain anlegen.
    UserCreate {
        domain_id: String,
        local_part: String,
        password: String,
        #[arg(long, default_value = "user")]
        role: String,
    },
    /// Benutzer einer Domain auflisten.
    UserList { domain_id: String },
    /// Diagnose: erreichbar & Datenbankstatus.
    Status,
}

#[derive(Serialize, Deserialize)]
struct Credentials {
    access_token: String,
    refresh_token: String,
}

fn credentials_path() -> PathBuf {
    let base = dirs_home().join(".config").join("havenmail");
    base.join("credentials.json")
}

/// Minimaler Ersatz für die `dirs`-Crate, um keine zusätzliche Abhängigkeit
/// nur für die Home-Verzeichnis-Auflösung einzuführen.
fn dirs_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn save_credentials(creds: &Credentials) -> std::io::Result<()> {
    let path = credentials_path();
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, serde_json::to_string_pretty(creds)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn load_access_token() -> Option<String> {
    let content = std::fs::read_to_string(credentials_path()).ok()?;
    let creds: Credentials = serde_json::from_str(&content).ok()?;
    Some(creds.access_token)
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let client = reqwest::Client::new();

    let result = match cli.command {
        Command::Login { email, password } => login(&client, &cli.api_url, &email, &password).await,
        Command::DomainCreate { name } => {
            authed_post(
                &client,
                &cli.api_url,
                "/api/v1/domains",
                json!({ "name": name }),
            )
            .await
        }
        Command::DomainList => authed_get(&client, &cli.api_url, "/api/v1/domains").await,
        Command::UserCreate {
            domain_id,
            local_part,
            password,
            role,
        } => {
            authed_post(
                &client,
                &cli.api_url,
                &format!("/api/v1/domains/{domain_id}/users"),
                json!({ "local_part": local_part, "password": password, "role": role }),
            )
            .await
        }
        Command::UserList { domain_id } => {
            authed_get(
                &client,
                &cli.api_url,
                &format!("/api/v1/domains/{domain_id}/users"),
            )
            .await
        }
        Command::Status => authed_get(&client, &cli.api_url, "/readyz").await,
    };

    match result {
        Ok(value) => println!("{}", serde_json::to_string_pretty(&value).unwrap()),
        Err(err) => {
            eprintln!("Fehler: {err}");
            std::process::exit(1);
        }
    }
}

async fn login(
    client: &reqwest::Client,
    api_url: &str,
    email: &str,
    password: &str,
) -> Result<Value, String> {
    let response = client
        .post(format!("{api_url}/api/v1/auth/login"))
        .json(&json!({ "email": email, "password": password }))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        return Err(format!("Login fehlgeschlagen ({})", response.status()));
    }
    let body: Value = response.json().await.map_err(|e| e.to_string())?;
    let creds = Credentials {
        access_token: body["access_token"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        refresh_token: body["refresh_token"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
    };
    save_credentials(&creds).map_err(|e| e.to_string())?;
    Ok(json!({ "status": "eingeloggt" }))
}

async fn authed_get(client: &reqwest::Client, api_url: &str, path: &str) -> Result<Value, String> {
    let token = load_access_token().ok_or("nicht angemeldet — zuerst `login` ausführen")?;
    let response = client
        .get(format!("{api_url}{path}"))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    response_to_value(response).await
}

async fn authed_post(
    client: &reqwest::Client,
    api_url: &str,
    path: &str,
    body: Value,
) -> Result<Value, String> {
    let token = load_access_token().ok_or("nicht angemeldet — zuerst `login` ausführen")?;
    let response = client
        .post(format!("{api_url}{path}"))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    response_to_value(response).await
}

async fn response_to_value(response: reqwest::Response) -> Result<Value, String> {
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    if !status.is_success() {
        return Err(format!("HTTP {status}: {body}"));
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_parses_status_subcommand() {
        let cli = Cli::parse_from(["havenmail-cli", "status"]);
        assert!(matches!(cli.command, Command::Status));
    }

    #[test]
    fn cli_parses_domain_create() {
        let cli = Cli::parse_from(["havenmail-cli", "domain-create", "example.org"]);
        match cli.command {
            Command::DomainCreate { name } => assert_eq!(name, "example.org"),
            _ => panic!("falscher Subcommand geparst"),
        }
    }

    #[test]
    fn cli_parses_custom_api_url() {
        let cli = Cli::parse_from([
            "havenmail-cli",
            "--api-url",
            "https://mail.example.org",
            "status",
        ]);
        assert_eq!(cli.api_url, "https://mail.example.org");
    }
}
