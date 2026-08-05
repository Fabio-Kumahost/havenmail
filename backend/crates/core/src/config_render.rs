//! Rendert die in `config/` hinterlegten Postfix-/Dovecot-/Rspamd-Templates
//! mit den tatsächlichen Werten einer Installation (DB-Zugangsdaten,
//! TLS-Zertifikatspfade, Hostname). Nutzt die etablierte `tera`-Templating-
//! Engine — es wird kein eigener Parser für Konfigurationsdateien geschrieben.
//!
//! Die gerenderten Dateien enthalten selbst keinerlei SMTP-/IMAP-/TLS-
//! Protokolllogik; sie konfigurieren nur, wie Postfix/Dovecot/Rspamd sich
//! verhalten (siehe docs/architecture.md, Abschnitt Architekturentscheidung).

use serde::Serialize;
use std::path::Path;
use tera::{Context, Tera};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("Templates konnten nicht geladen werden: {0}")]
    Load(String),
    #[error("Template '{template}' konnte nicht gerendert werden: {source}")]
    Render {
        template: String,
        source: tera::Error,
    },
}

/// Werte, die in alle Havenmail-Konfigurationstemplates eingesetzt werden.
/// Erweitert sich mit M3 (TLS/DKIM) und M5 (Installer) um weitere Felder.
#[derive(Debug, Clone, Serialize)]
pub struct RenderContext {
    pub mail_hostname: String,
    pub db_host: String,
    pub db_port: u16,
    pub db_name: String,
    pub db_user: String,
    /// Wird nie ins Klartext-Logging aufgenommen, nur ins gerenderte File
    /// (das mit restriktiven Dateirechten geschrieben wird, siehe M5).
    pub db_password: String,
    pub tls_cert_path: String,
    pub tls_key_path: String,
    /// Verzeichnis mit dem gebauten Frontend (`vite build`-Ausgabe), das
    /// nginx als statische Dateien ausliefert (M5).
    pub frontend_dist_dir: String,
    /// Adresse, an die die Control-Plane-API gebunden ist (`host:port`);
    /// nginx reverse-proxied `/api/`, `/healthz`, `/readyz` dorthin (M5).
    pub api_bind: String,
}

/// Lädt alle `*.tera`-Templates aus `config_dir` (rekursiv) und rendert
/// `template_name` (Pfad relativ zu `config_dir`, z. B. `"postfix/main.cf.tera"`).
pub fn render_template(
    config_dir: &Path,
    template_name: &str,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    render_with(config_dir, template_name, ctx)
}

/// Werte für die vom Admin-Panel editierbaren Rspamd-Einstellungen
/// (`security_settings`-Tabelle ist die Quelle der Wahrheit, siehe
/// routes/security_settings.rs). Bewusst ein eigener, kleinerer Context
/// statt `RenderContext` zu erweitern — diese Felder haben nichts mit
/// Install-Zeit-Werten (DB-Zugangsdaten, TLS-Pfade) zu tun und werden bei
/// jeder Einstellungsänderung neu gerendert, nicht nur einmalig beim Install.
#[derive(Debug, Clone, Serialize)]
pub struct SecurityRenderContext {
    pub spam_greylist_score: f32,
    pub spam_add_header_score: f32,
    pub spam_reject_score: f32,
    pub dmarc_enabled: bool,
    pub ratelimit_enabled: bool,
    pub ratelimit_per_hour: i32,
    pub ratelimit_burst: i32,
    pub antivirus_enabled: bool,
    pub antivirus_action: String,
    pub antivirus_max_size_mb: i32,
}

/// Rendert genau die vier Rspamd-Templates, die von `security_settings`
/// abhängen. Rückgabe: Template-Name (relativ zu `config_dir`) -> Inhalt,
/// damit der Aufrufer selbst entscheidet, wohin welche Datei geschrieben
/// wird (siehe routes/security_settings.rs).
pub fn render_security_settings(
    config_dir: &Path,
    ctx: &SecurityRenderContext,
) -> Result<Vec<(&'static str, String)>, RenderError> {
    const TEMPLATES: &[&str] = &[
        "rspamd/local.d/actions.conf.tera",
        "rspamd/local.d/antivirus.conf.tera",
        "rspamd/local.d/dmarc.conf.tera",
        "rspamd/local.d/ratelimit.conf.tera",
    ];

    TEMPLATES
        .iter()
        .map(|name| Ok((*name, render_with(config_dir, name, ctx)?)))
        .collect()
}

fn render_with<T: Serialize>(
    config_dir: &Path,
    template_name: &str,
    ctx: &T,
) -> Result<String, RenderError> {
    let glob = format!("{}/**/*.tera", config_dir.display());
    let tera = Tera::new(&glob).map_err(|e| RenderError::Load(e.to_string()))?;

    let tera_context = Context::from_serialize(ctx).map_err(|e| RenderError::Render {
        template: template_name.to_string(),
        source: e,
    })?;

    tera.render(template_name, &tera_context)
        .map_err(|e| RenderError::Render {
            template: template_name.to_string(),
            source: e,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_context() -> RenderContext {
        RenderContext {
            mail_hostname: "mail.example.org".to_string(),
            db_host: "127.0.0.1".to_string(),
            db_port: 5432,
            db_name: "havenmail".to_string(),
            db_user: "havenmail".to_string(),
            db_password: "test-secret".to_string(),
            tls_cert_path: "/etc/havenmail/tls/fullchain.pem".to_string(),
            tls_key_path: "/etc/havenmail/tls/privkey.pem".to_string(),
            frontend_dist_dir: "/opt/havenmail/frontend/dist".to_string(),
            api_bind: "127.0.0.1:8080".to_string(),
        }
    }

    fn repo_config_dir() -> std::path::PathBuf {
        // backend/crates/core -> ../../../config (Repo-Root/config)
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../config")
    }

    #[test]
    fn renders_postfix_virtual_mailbox_maps_with_db_values() {
        let out = render_template(
            &repo_config_dir(),
            "postfix/pgsql-virtual-mailboxes.cf.tera",
            &sample_context(),
        )
        .expect("Rendering sollte gelingen");
        assert!(out.contains("dbname = havenmail"));
        assert!(out.contains("hosts = 127.0.0.1:5432"));
        assert!(out.contains("postfix_virtual_mailboxes"));
    }

    #[test]
    fn renders_dovecot_sql_conf_with_hostname() {
        let out = render_template(
            &repo_config_dir(),
            "dovecot/dovecot-sql.conf.ext.tera",
            &sample_context(),
        )
        .expect("Rendering sollte gelingen");
        assert!(out.contains("dovecot_auth_users"));
    }

    /// Regressionstest: `havenmail-cli render-configs` (Install-Zeit) rendert
    /// ALLE `*.tera`-Dateien unter config/ mit dem allgemeinen
    /// `RenderContext`, der die security_settings-Felder nicht kennt. Ohne
    /// `default(value=...)` in den vier betroffenen Templates würde das
    /// hier mit einem Tera-Fehler ("undefined variable") fehlschlagen.
    #[test]
    fn security_templates_render_under_generic_install_time_context() {
        for template in [
            "rspamd/local.d/actions.conf.tera",
            "rspamd/local.d/antivirus.conf.tera",
            "rspamd/local.d/dmarc.conf.tera",
            "rspamd/local.d/ratelimit.conf.tera",
        ] {
            let out = render_template(&repo_config_dir(), template, &sample_context())
                .unwrap_or_else(|e| panic!("{template} sollte unter RenderContext rendern: {e}"));
            assert!(!out.trim().is_empty());
        }
    }

    #[test]
    fn renders_rspamd_dkim_signing_config_with_hostname() {
        let out = render_template(
            &repo_config_dir(),
            "rspamd/local.d/dkim_signing.conf.tera",
            &sample_context(),
        )
        .expect("Rendering sollte gelingen");
        assert!(out.contains("mail.example.org") || out.contains("selector_map"));
    }

    #[test]
    fn renders_nginx_bootstrap_vhost_with_hostname() {
        let out = render_template(
            &repo_config_dir(),
            "nginx/havenmail-http.conf.tera",
            &sample_context(),
        )
        .expect("Rendering sollte gelingen");
        assert!(out.contains("server_name mail.example.org"));
        assert!(out.contains("acme-challenge"));
        assert!(!out.contains("ssl_certificate"));
    }

    #[test]
    fn renders_nginx_full_vhost_with_tls_and_proxy_target() {
        let out = render_template(&repo_config_dir(), "nginx/havenmail.conf.tera", &sample_context())
            .expect("Rendering sollte gelingen");
        assert!(out.contains("ssl_certificate /etc/havenmail/tls/fullchain.pem"));
        assert!(out.contains("proxy_pass http://127.0.0.1:8080"));
        assert!(out.contains("root /opt/havenmail/frontend/dist"));
    }

    #[test]
    fn unknown_template_yields_render_error() {
        let result = render_template(&repo_config_dir(), "does/not/exist.tera", &sample_context());
        assert!(result.is_err());
    }

    fn sample_security_context() -> SecurityRenderContext {
        SecurityRenderContext {
            spam_greylist_score: 4.0,
            spam_add_header_score: 6.0,
            spam_reject_score: 15.0,
            dmarc_enabled: true,
            ratelimit_enabled: true,
            ratelimit_per_hour: 100,
            ratelimit_burst: 100,
            antivirus_enabled: true,
            antivirus_action: "reject".to_string(),
            antivirus_max_size_mb: 25,
        }
    }

    #[test]
    fn renders_all_four_security_templates() {
        let rendered = render_security_settings(&repo_config_dir(), &sample_security_context())
            .expect("Rendering sollte gelingen");
        assert_eq!(rendered.len(), 4);

        let actions = &rendered
            .iter()
            .find(|(name, _)| *name == "rspamd/local.d/actions.conf.tera")
            .unwrap()
            .1;
        assert!(actions.contains("greylist = 4"));
        assert!(actions.contains("reject = 15"));

        let antivirus = &rendered
            .iter()
            .find(|(name, _)| *name == "rspamd/local.d/antivirus.conf.tera")
            .unwrap()
            .1;
        assert!(antivirus.contains("enabled = true"));
        assert!(antivirus.contains("action = \"reject\""));
        assert!(antivirus.contains("max_size = 25m"));
    }

    #[test]
    fn antivirus_action_line_omitted_when_not_reject() {
        let mut ctx = sample_security_context();
        ctx.antivirus_action = "add_header".to_string();
        let rendered = render_security_settings(&repo_config_dir(), &ctx).unwrap();
        let antivirus = &rendered
            .iter()
            .find(|(name, _)| *name == "rspamd/local.d/antivirus.conf.tera")
            .unwrap()
            .1;
        assert!(!antivirus.contains("action ="));
    }
}
