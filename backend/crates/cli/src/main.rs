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
        #[arg(long, default_value = "/opt/havenmail/frontend/dist")]
        frontend_dist_dir: String,
        #[arg(long, env = "HAVENMAIL_API_BIND", default_value = "127.0.0.1:8080")]
        api_bind: String,
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
    /// Erfasst eine Momentaufnahme der Rspamd-/ClamAV-/Postfix-Kennzahlen
    /// für die Dashboard-Verlaufscharts. Verbindet sich direkt mit der
    /// Datenbank statt über die API (kein Authentifizierungs-Overhead für
    /// einen von systemd getriggerten Dienstkonto-Job nötig, gleiche
    /// Begründung wie bei `BootstrapAdmin`). Vom systemd-Timer
    /// `havenmail-metrics-snapshot.timer` alle 15 Minuten aufgerufen.
    SnapshotMetrics {
        #[arg(long, env = "DATABASE_URL")]
        database_url: String,
        #[arg(long, default_value = "/var/log/clamav/clamav.log")]
        clamav_log_path: PathBuf,
        #[arg(long, default_value = "/var/lib/clamav")]
        clamav_lib_dir: PathBuf,
        #[arg(long, default_value = "/var/mail")]
        mail_spool_path: PathBuf,
    },
    /// Fragt die konfigurierten Fail2Ban-Jails ab und schreibt den
    /// aktuellen Sperr-Status als JSON (Admin-Panel liest nur diese
    /// Datei, kein direkter fail2ban-Socket-Zugriff — siehe
    /// havenmail_core::fail2ban). Läuft als root über
    /// havenmail-fail2ban-status.timer (alle 30s).
    Fail2banRefreshStatus {
        #[arg(long, default_value = "/var/lib/havenmail/fail2ban-status.json")]
        status_file: PathBuf,
    },
    /// Entsperrt eine IP in einem Jail. `--request-file` enthält
    /// "<jail>:<ip>" (von der API validiert und geschrieben, siehe
    /// routes/fail2ban.rs) — läuft als root über
    /// havenmail-fail2ban-unban.path, ausgelöst durch die Trigger-Datei.
    /// Aktualisiert den Status danach gleich mit, damit der nächste
    /// Panel-Abruf die Entsperrung ohne Wartezeit auf den nächsten
    /// Timer-Tick zeigt.
    Fail2banUnban {
        #[arg(long)]
        request_file: PathBuf,
        #[arg(long, default_value = "/var/lib/havenmail/fail2ban-status.json")]
        status_file: PathBuf,
    },
    /// Führt backup.sh aus und schreibt Ergebnis + Archiv-Historie als
    /// JSON (Admin-Panel liest nur diese Datei — backup.sh selbst braucht
    /// root, u. a. für den Lesezugriff auf /etc/havenmail inkl.
    /// HAVENMAIL_SECRETS_KEY, das kann nicht an den havenmail-
    /// Systembenutzer delegiert werden wie die anderen Trigger-Aktionen).
    /// Läuft sowohl manuell getriggert (havenmail-backup-trigger.path)
    /// als auch automatisch (havenmail-backup.timer, täglich) über
    /// denselben Service.
    BackupRun {
        #[arg(long, default_value = "/opt/havenmail/backup.sh")]
        backup_script: PathBuf,
        #[arg(long, default_value = "/var/backups/havenmail")]
        backup_dir: PathBuf,
        #[arg(long, default_value = "/var/lib/havenmail/backup-status.json")]
        status_file: PathBuf,
    },
    /// Prüft kritische Zustände (TLS-Ablauf, Speicherplatz, Dienststatus je
    /// orchestriertem Dienst, letztes Backup) und schickt bei
    /// Zustandswechseln bzw. andauernden Problemen (Erinnerung höchstens
    /// alle `remind_after_hours`) eine E-Mail an HAVENMAIL_ADMIN_EMAIL.
    /// Verbindet sich direkt mit der Datenbank (notification_state,
    /// metrics_snapshots), gleiche Begründung wie SnapshotMetrics. Läuft
    /// alle 5 Minuten über havenmail-notify-check.timer.
    NotifyCheck {
        #[arg(long, env = "DATABASE_URL")]
        database_url: String,
        #[arg(long, env = "HAVENMAIL_ADMIN_EMAIL")]
        admin_email: Option<String>,
        #[arg(long, env = "HAVENMAIL_ETC_DIR", default_value = "/etc/havenmail")]
        etc_dir: PathBuf,
        #[arg(long, default_value = "/var/lib/havenmail/backup-status.json")]
        backup_status_file: PathBuf,
        #[arg(long, default_value_t = 14)]
        tls_warn_days: i64,
        #[arg(long, default_value_t = 90.0)]
        disk_warn_percent: f32,
        #[arg(long, default_value_t = 24)]
        remind_after_hours: i64,
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
            frontend_dist_dir,
            api_bind,
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
                frontend_dist_dir,
                api_bind,
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
        Command::SnapshotMetrics {
            database_url,
            clamav_log_path,
            clamav_lib_dir,
            mail_spool_path,
        } => snapshot_metrics(&database_url, &clamav_log_path, &clamav_lib_dir, &mail_spool_path)
            .await
            .map_err(|e| e.to_string()),
        Command::Fail2banRefreshStatus { status_file } => {
            havenmail_core::fail2ban::refresh_status(&status_file)
                .await
                .map(|s| json!({ "status": "refreshed", "jails": s.jails.len() }))
                .map_err(|e| e.to_string())
        }
        Command::Fail2banUnban {
            request_file,
            status_file,
        } => fail2ban_unban(&request_file, &status_file)
            .await
            .map_err(|e| e.to_string()),
        Command::BackupRun {
            backup_script,
            backup_dir,
            status_file,
        } => backup_run(&backup_script, &backup_dir, &status_file)
            .await
            .map_err(|e| e.to_string()),
        Command::NotifyCheck {
            database_url,
            admin_email,
            etc_dir,
            backup_status_file,
            tls_warn_days,
            disk_warn_percent,
            remind_after_hours,
        } => notify_check(
            &database_url,
            admin_email.as_deref(),
            &etc_dir,
            &backup_status_file,
            tls_warn_days,
            disk_warn_percent,
            chrono::Duration::hours(remind_after_hours),
        )
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

/// Sammelt eine Momentaufnahme aus Rspamd, ClamAV-Log und Postfix-Queue
/// und schreibt sie in `metrics_snapshots`. Jeder einzelne Sammelschritt
/// ist bestmöglich-tolerant (liefert `None`/überspringt bei Fehler) — ein
/// ausgefallener Dienst (z. B. Rspamd kurz neu gestartet) darf den
/// gesamten Snapshot nicht verhindern, siehe havenmail_core::system.rs
/// (`query_unit_status`) für denselben defensiven Stil.
async fn snapshot_metrics(
    database_url: &str,
    clamav_log_path: &std::path::Path,
    clamav_lib_dir: &std::path::Path,
    mail_spool_path: &std::path::Path,
) -> Result<Value, String> {
    let pool = havenmail_core::db::connect(database_url)
        .await
        .map_err(|e| e.to_string())?;

    let last_captured_at: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT captured_at FROM metrics_snapshots ORDER BY captured_at DESC LIMIT 1")
            .fetch_optional(&pool)
            .await
            .map_err(|e| e.to_string())?;
    let clamav_since = last_captured_at.unwrap_or_else(|| chrono::Utc::now() - chrono::Duration::hours(1));

    let stat = havenmail_core::rspamd_client::RspamdClient::default()
        .stat()
        .await
        .ok();
    let clamav_detected = havenmail_core::clamav_stats::detected_since(clamav_log_path, clamav_since);
    let signature_age = havenmail_core::clamav_stats::signature_age(clamav_lib_dir);
    let queue_size = havenmail_core::mail_queue::queue_size().await;
    let disk_used_percent = disk_used_percent(mail_spool_path).await;
    // Gleicher "since"-Anker wie ClamAV: Zeitraum seit dem letzten
    // Snapshot (bzw. 1h zurück beim allerersten Lauf).
    let (mail_sent, mail_received) = havenmail_core::mail_flow::counts_since(clamav_since).await;

    sqlx::query(
        r#"
        INSERT INTO metrics_snapshots (
            rspamd_scanned, rspamd_spam_count, rspamd_ham_count,
            rspamd_action_reject, rspamd_action_add_header, rspamd_action_greylist, rspamd_action_no_action,
            clamav_detected_since_last, clamav_signature_age_hours,
            mail_queue_size, disk_used_percent,
            mail_sent_count, mail_received_count
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
        "#,
    )
    .bind(stat.as_ref().map(|s| s.scanned as i64))
    .bind(stat.as_ref().map(|s| s.spam_count as i64))
    .bind(stat.as_ref().map(|s| s.ham_count as i64))
    .bind(stat.as_ref().map(|s| s.actions.reject as i64))
    .bind(stat.as_ref().map(|s| s.actions.add_header as i64))
    .bind(stat.as_ref().map(|s| s.actions.greylist as i64))
    .bind(stat.as_ref().map(|s| s.actions.no_action as i64))
    .bind(clamav_detected as i32)
    .bind(signature_age.map(|h| h as i32))
    .bind(queue_size.map(|q| q as i32))
    .bind(disk_used_percent)
    .bind(mail_sent as i32)
    .bind(mail_received as i32)
    .execute(&pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(json!({
        "status": "recorded",
        "rspamd_reachable": stat.is_some(),
        "clamav_detected_since_last": clamav_detected,
        "mail_queue_size": queue_size,
        "disk_used_percent": disk_used_percent,
        "mail_sent": mail_sent,
        "mail_received": mail_received,
    }))
}

/// `df --output=pcent <path>` gibt eine Kopfzeile ("Use%") gefolgt von
/// einer rechtsbündigen Prozentzahl mit "%"-Suffix aus (z. B. " 12%").
async fn disk_used_percent(path: &std::path::Path) -> Option<f32> {
    let output = tokio::process::Command::new("df")
        .args(["--output=pcent"])
        .arg(path)
        .output()
        .await
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value_line = stdout.lines().nth(1)?;
    value_line.trim().trim_end_matches('%').parse().ok()
}

/// Liest "<jail>:<ip>" aus `request_file` (von der API geschrieben, dort
/// bereits validiert — hier zusätzlich über
/// `havenmail_core::fail2ban::unban` erneut geprüft, statt der API-Seite
/// blind zu vertrauen), entsperrt und aktualisiert danach den Status.
async fn fail2ban_unban(
    request_file: &std::path::Path,
    status_file: &std::path::Path,
) -> Result<Value, String> {
    let content = std::fs::read_to_string(request_file).map_err(|e| e.to_string())?;
    let (jail, ip) = content
        .trim()
        .split_once(':')
        .ok_or("Trigger-Datei-Inhalt muss '<jail>:<ip>' sein")?;

    let unbanned = havenmail_core::fail2ban::unban(jail, ip)
        .await
        .map_err(|e| e.to_string())?;

    havenmail_core::fail2ban::refresh_status(status_file)
        .await
        .map_err(|e| e.to_string())?;

    Ok(json!({ "status": if unbanned { "unbanned" } else { "failed" }, "jail": jail, "ip": ip }))
}

/// Führt `backup_script` (backup.sh) aus und schreibt Ergebnis +
/// Archiv-Historie nach `status_file`. Läuft als root (backup.sh braucht
/// das selbst, siehe havenmail_require_root darin) — das Admin-Panel
/// bekommt nie direkten Zugriff auf die Backup-Archive (enthalten
/// HAVENMAIL_SECRETS_KEY, DB-Dump, Maildaten), nur auf diese Metadaten.
async fn backup_run(
    backup_script: &std::path::Path,
    backup_dir: &std::path::Path,
    status_file: &std::path::Path,
) -> Result<Value, String> {
    let output = tokio::process::Command::new(backup_script)
        .output()
        .await
        .map_err(|e| e.to_string())?;

    let success = output.status.success();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    // backup.sh gibt den fertigen Archivpfad als letzte Zeile auf stdout
    // aus (`echo "$archive"` am Ende von havenmail_backup_create) — alle
    // havenmail_log-Meldungen gehen nach stderr.
    let archive = stdout.lines().last().unwrap_or("").to_string();

    let history = list_backup_history(backup_dir);
    let ran_at = chrono::Utc::now();

    let last_run = if success {
        json!({ "status": "success", "ran_at": ran_at, "archive": archive })
    } else {
        json!({
            "status": "failed",
            "ran_at": ran_at,
            "error": if stderr.is_empty() { "unbekannter Fehler".to_string() } else { stderr.clone() },
        })
    };

    let status = json!({ "last_run": last_run, "history": history });
    std::fs::write(
        status_file,
        serde_json::to_string_pretty(&status).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    if !success {
        return Err(format!("backup.sh fehlgeschlagen: {stderr}"));
    }
    Ok(status)
}

/// Listet vorhandene Archive nach Havenmails eigenem Namensschema
/// (`havenmail-<timestamp>.tar.gz[.gpg]`, siehe backup_steps.sh) — die
/// Historie ist damit immer die tatsächliche Plattenrealität, keine
/// separat gepflegte und potenziell veraltende Logdatei. Retention-
/// Löschungen durch backup.sh spiegeln sich automatisch wider.
fn list_backup_history(backup_dir: &std::path::Path) -> Vec<Value> {
    let Ok(entries) = std::fs::read_dir(backup_dir) else {
        return Vec::new();
    };
    let mut items: Vec<(String, u64, std::time::SystemTime)> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.starts_with("havenmail-") || !name.contains(".tar.gz") {
                return None;
            }
            let meta = e.metadata().ok()?;
            Some((name, meta.len(), meta.modified().ok()?))
        })
        .collect();
    items.sort_by(|a, b| b.2.cmp(&a.2));
    items
        .into_iter()
        .map(|(name, size, mtime)| {
            let created_at: chrono::DateTime<chrono::Utc> = mtime.into();
            json!({ "filename": name, "size_bytes": size, "created_at": created_at })
        })
        .collect()
}

/// Prüft TLS-Ablauf, Speicherplatz, jeden orchestrierten Dienst sowie das
/// letzte Backup-Ergebnis; verschickt bei Bedarf eine Admin-Mail (siehe
/// `havenmail_core::notify::decide_notification`) und protokolliert jede
/// verschickte Benachrichtigung zusätzlich im Audit-Log (`actor_id` NULL —
/// eindeutig als Systemereignis statt Admin-Aktion erkennbar).
#[allow(clippy::too_many_arguments)]
async fn notify_check(
    database_url: &str,
    admin_email: Option<&str>,
    etc_dir: &std::path::Path,
    backup_status_file: &std::path::Path,
    tls_warn_days: i64,
    disk_warn_percent: f32,
    remind_after: chrono::Duration,
) -> Result<Value, String> {
    let pool = havenmail_core::db::connect(database_url)
        .await
        .map_err(|e| e.to_string())?;

    let admin_email = match admin_email {
        Some(email) if !email.is_empty() => email,
        _ => {
            return Ok(json!({
                "status": "übersprungen",
                "grund": "HAVENMAIL_ADMIN_EMAIL nicht gesetzt",
            }))
        }
    };

    let mut results = Vec::new();

    // TLS-Ablauf
    let etc_dir_str = etc_dir.to_string_lossy();
    if let Some(tls) = havenmail_core::tls_status::read(&etc_dir_str) {
        let (status, message) = match tls.days_remaining {
            Some(days) if days < tls_warn_days => (
                havenmail_core::notify::CheckStatus::Problem,
                format!("TLS-Zertifikat läuft in {days} Tagen ab ({}).", tls.expires_at),
            ),
            _ => (
                havenmail_core::notify::CheckStatus::Ok,
                format!("TLS-Zertifikat gültig bis {}.", tls.expires_at),
            ),
        };
        results.push(
            process_check(&pool, "tls_expiry", "TLS-Zertifikat", status, &message, admin_email, remind_after)
                .await?,
        );
    }

    // Speicherplatz (letzter Snapshot aus metrics_snapshots, alle 15min
    // via havenmail-cli snapshot-metrics befüllt — kein eigener df-Aufruf
    // hier nötig).
    let disk_percent: Option<f32> = sqlx::query_scalar::<_, Option<f32>>(
        "SELECT disk_used_percent FROM metrics_snapshots ORDER BY captured_at DESC LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .map_err(|e| e.to_string())?
    .flatten();
    if let Some(percent) = disk_percent {
        let status = if percent >= disk_warn_percent {
            havenmail_core::notify::CheckStatus::Problem
        } else {
            havenmail_core::notify::CheckStatus::Ok
        };
        let message = format!("Speicherauslastung (Maildaten): {percent:.1}%.");
        results.push(
            process_check(&pool, "disk_usage", "Speicherplatz", status, &message, admin_email, remind_after)
                .await?,
        );
    }

    // Dienststatus — ein Check pro Dienst, damit ein einzelner Ausfall
    // gezielt gemeldet UND einzeln als "behoben" quittiert werden kann.
    for unit in havenmail_core::service_status::MANAGED_UNITS {
        let service = havenmail_core::service_status::query_unit_status(unit).await;
        let status = if service.active {
            havenmail_core::notify::CheckStatus::Ok
        } else {
            havenmail_core::notify::CheckStatus::Problem
        };
        let message = format!("Dienst {unit}: {}.", service.detail);
        let check_key = format!("service:{unit}");
        let label = format!("Dienst {unit}");
        results.push(process_check(&pool, &check_key, &label, status, &message, admin_email, remind_after).await?);
    }

    // Letztes Backup (Status-Datei von havenmail-cli backup-run, siehe
    // routes/backup.rs — kein direkter DB-Zugriff auf backup_runs nötig).
    if let Ok(content) = std::fs::read_to_string(backup_status_file) {
        if let Ok(value) = serde_json::from_str::<Value>(&content) {
            if let Some(last_run) = value.get("last_run").filter(|v| !v.is_null()) {
                let run_status = last_run.get("status").and_then(|v| v.as_str()).unwrap_or("");
                let (status, message) = if run_status == "failed" {
                    let error = last_run
                        .get("error")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unbekannter Fehler");
                    (
                        havenmail_core::notify::CheckStatus::Problem,
                        format!("Letztes Backup fehlgeschlagen: {error}"),
                    )
                } else {
                    (
                        havenmail_core::notify::CheckStatus::Ok,
                        "Letztes Backup erfolgreich.".to_string(),
                    )
                };
                results.push(
                    process_check(&pool, "backup", "Backup", status, &message, admin_email, remind_after).await?,
                );
            }
        }
    }

    let sent = results
        .iter()
        .filter(|r| r["sent"].as_bool().unwrap_or(false))
        .count();
    Ok(json!({
        "status": "geprüft",
        "checks": results.len(),
        "benachrichtigungen_verschickt": sent,
        "details": results,
    }))
}

/// Verarbeitet einen einzelnen Check: liest den vorherigen Zustand aus
/// `notification_state`, entscheidet per `decide_notification`, verschickt
/// bei Bedarf eine Mail, protokolliert sie im Audit-Log und schreibt den
/// neuen Zustand zurück.
#[allow(clippy::too_many_arguments)]
async fn process_check(
    pool: &sqlx::PgPool,
    check_key: &str,
    label: &str,
    status: havenmail_core::notify::CheckStatus,
    message: &str,
    admin_email: &str,
    remind_after: chrono::Duration,
) -> Result<Value, String> {
    let now = chrono::Utc::now();

    let previous: Option<(String, Option<chrono::DateTime<chrono::Utc>>)> =
        sqlx::query_as("SELECT status, last_notified_at FROM notification_state WHERE check_key = $1")
            .bind(check_key)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;

    let previous_decoded = previous.as_ref().map(|(s, t)| {
        (
            havenmail_core::notify::CheckStatus::from_db_str(s)
                .unwrap_or(havenmail_core::notify::CheckStatus::Ok),
            *t,
        )
    });

    let kind = havenmail_core::notify::decide_notification(status, previous_decoded, now, remind_after);

    let mut sent = false;
    if let Some(kind) = kind {
        let (prefix, intro) = match kind {
            havenmail_core::notify::NotifyKind::Alert => {
                ("ALARM", format!("Für „{label}“ liegt ein neues Problem vor."))
            }
            havenmail_core::notify::NotifyKind::Reminder => (
                "Erinnerung",
                format!("Für „{label}“ besteht das Problem weiterhin."),
            ),
            havenmail_core::notify::NotifyKind::Resolved => {
                ("Behoben", format!("Das Problem bei „{label}“ ist behoben."))
            }
        };
        let subject = format!("[Havenmail] {prefix}: {label}");
        let body = format!("{intro}\n\n{message}\n\n— Havenmail Systembenachrichtigung");
        match havenmail_core::notify::send_admin_email(admin_email, &subject, &body).await {
            Ok(()) => {
                sent = true;
                let action = match kind {
                    havenmail_core::notify::NotifyKind::Alert => "notify.alert",
                    havenmail_core::notify::NotifyKind::Reminder => "notify.reminder",
                    havenmail_core::notify::NotifyKind::Resolved => "notify.resolved",
                };
                if let Err(err) = havenmail_core::audit::record(
                    pool,
                    None,
                    action,
                    check_key,
                    None,
                    None,
                    Some(json!({ "message": message })),
                    None,
                )
                .await
                {
                    eprintln!("Warnung: Audit-Log-Eintrag für {check_key} fehlgeschlagen: {err}");
                }
            }
            Err(err) => {
                eprintln!("Warnung: Benachrichtigung für {check_key} konnte nicht verschickt werden: {err}");
            }
        }
    }

    let first_seen_at = match &previous {
        Some((prev_status, _)) if prev_status.as_str() == status.as_db_str() => None,
        _ => Some(now),
    };
    let last_notified_at = if sent {
        Some(now)
    } else {
        previous.as_ref().and_then(|(_, t)| *t)
    };

    sqlx::query(
        r#"
        INSERT INTO notification_state (check_key, status, message, first_seen_at, last_notified_at, updated_at)
        VALUES ($1, $2, $3, COALESCE($4, now()), $5, now())
        ON CONFLICT (check_key) DO UPDATE SET
            status = EXCLUDED.status,
            message = EXCLUDED.message,
            first_seen_at = COALESCE($4, notification_state.first_seen_at),
            last_notified_at = EXCLUDED.last_notified_at,
            updated_at = now()
        "#,
    )
    .bind(check_key)
    .bind(status.as_db_str())
    .bind(message)
    .bind(first_seen_at)
    .bind(last_notified_at)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(json!({
        "check": check_key,
        "status": status.as_db_str(),
        "sent": sent,
    }))
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
