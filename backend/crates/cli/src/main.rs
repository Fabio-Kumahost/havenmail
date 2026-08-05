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
    /// Rendert die Postfix-/Dovecot-/Rspamd-Konfigurationstemplates aus
    /// `config/` mit den übergebenen Werten nach `--out-dir` (lokal, ohne
    /// API-Zugriff — vom Installer genutzt, siehe scripts/lib/install_steps.sh).
    RenderConfigs {
        /// Wurzelverzeichnis mit den `*.tera`-Templates (Repo-`config/`-Ordner).
        #[arg(long)]
        config_dir: PathBuf,
        /// Zielverzeichnis für die gerenderten Dateien.
        #[arg(long)]
        out_dir: PathBuf,
        #[arg(long, env = "HAVENMAIL_HOSTNAME")]
        mail_hostname: String,
        #[arg(long, default_value = "127.0.0.1")]
        db_host: String,
        #[arg(long, default_value_t = 5432)]
        db_port: u16,
        #[arg(long, default_value = "havenmail")]
        db_name: String,
        #[arg(long, default_value = "havenmail")]
        db_user: String,
        #[arg(long, env = "HAVENMAIL_DB_PASSWORD")]
        db_password: String,
        #[arg(long, default_value = "/etc/havenmail/tls/fullchain.pem")]
        tls_cert_path: String,
        #[arg(long, default_value = "/etc/havenmail/tls/privkey.pem")]
        tls_key_path: String,
    },
    /// Legt (idempotent) die erste Domain und deren `super_admin`-Konto an.
    /// Spricht direkt die Datenbank an, nicht die API — es gibt bewusst
    /// keinen unauthentifizierten API-Weg dafür (siehe
    /// havenmail_core::bootstrap). Vom Installer nach dem ersten Dienststart
    /// genutzt (scripts/lib/install_steps.sh).
    BootstrapAdmin {
        #[arg(long, env = "DATABASE_URL")]
        database_url: String,
        #[arg(long)]
        domain: String,
        /// Lokaler Teil der Admin-Adresse, z. B. "admin" für admin@domain.
        #[arg(long, default_value = "admin")]
        local_part: String,
        #[arg(long)]
        password: String,
    },
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
        Command::RenderConfigs {
            config_dir,
            out_dir,
            mail_hostname,
            db_host,
            db_port,
            db_name,
            db_user,
            db_password,
            tls_cert_path,
            tls_key_path,
        } => render_configs(
            &config_dir,
            &out_dir,
            havenmail_core::config_render::RenderContext {
                mail_hostname,
                db_host,
                db_port,
                db_name,
                db_user,
                db_password,
                tls_cert_path,
                tls_key_path,
            },
        )
        .map_err(|e| e.to_string()),
        Command::BootstrapAdmin {
            database_url,
            domain,
            local_part,
            password,
        } => bootstrap_admin(&database_url, &domain, &local_part, &password)
            .await
            .map_err(|e| e.to_string()),
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

/// Rendert alle Templates unter `config_dir` nach `out_dir` (Verzeichnisstruktur
/// bleibt erhalten, `.tera`-Endung entfällt). Nutzt die bereits getestete
/// `havenmail_core::config_render`-Logik (siehe M1) — keine eigene
/// Template-Engine.
fn render_configs(
    config_dir: &std::path::Path,
    out_dir: &std::path::Path,
    ctx: havenmail_core::config_render::RenderContext,
) -> Result<Value, havenmail_core::config_render::RenderError> {
    let templates = find_relative_templates(config_dir)
        .map_err(|e| havenmail_core::config_render::RenderError::Load(e.to_string()))?;

    let mut rendered = Vec::new();
    for rel in &templates {
        let template_name = rel.to_string_lossy().replace('\\', "/");
        let output =
            havenmail_core::config_render::render_template(config_dir, &template_name, &ctx)?;

        let out_path = out_dir.join(rel.with_extension(""));
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| havenmail_core::config_render::RenderError::Load(e.to_string()))?;
        }
        std::fs::write(&out_path, output)
            .map_err(|e| havenmail_core::config_render::RenderError::Load(e.to_string()))?;
        rendered.push(out_path.display().to_string());
    }
    Ok(json!({ "rendered": rendered }))
}

fn find_relative_templates(config_dir: &std::path::Path) -> std::io::Result<Vec<PathBuf>> {
    fn walk(
        base: &std::path::Path,
        dir: &std::path::Path,
        out: &mut Vec<PathBuf>,
    ) -> std::io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                walk(base, &path, out)?;
            } else if path.extension().and_then(|e| e.to_str()) == Some("tera") {
                out.push(path.strip_prefix(base).unwrap().to_path_buf());
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(config_dir, config_dir, &mut out)?;
    Ok(out)
}

/// Verbindet sich direkt mit der Datenbank (kein API-Token nötig — beim
/// allerersten Lauf existiert noch kein Konto, das sich anmelden könnte)
/// und legt Domain + super_admin idempotent an. Gibt den generierten
/// Zugang niemals über stdout in Klartext-Log-Dateien aus, die von einem
/// CI/Terminal-Recorder erfasst werden könnten — der Aufrufer (install.sh)
/// ist dafür verantwortlich, die Ausgabe angemessen zu behandeln.
async fn bootstrap_admin(
    database_url: &str,
    domain: &str,
    local_part: &str,
    password: &str,
) -> Result<Value, String> {
    let pool = havenmail_core::db::connect(database_url)
        .await
        .map_err(|e| e.to_string())?;
    havenmail_core::db::run_migrations(&pool)
        .await
        .map_err(|e| e.to_string())?;

    let outcome =
        havenmail_core::bootstrap::bootstrap_super_admin(&pool, domain, local_part, password)
            .await
            .map_err(|e| e.to_string())?;

    Ok(match outcome {
        havenmail_core::bootstrap::BootstrapOutcome::Created { domain_id, user_id } => json!({
            "status": "created",
            "domain_id": domain_id,
            "user_id": user_id,
        }),
        havenmail_core::bootstrap::BootstrapOutcome::AlreadyExists { domain_id, user_id } => json!({
            "status": "already_exists",
            "domain_id": domain_id,
            "user_id": user_id,
        }),
    })
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
