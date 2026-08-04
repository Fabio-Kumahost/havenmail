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
}

/// Lädt alle `*.tera`-Templates aus `config_dir` (rekursiv) und rendert
/// `template_name` (Pfad relativ zu `config_dir`, z. B. `"postfix/main.cf.tera"`).
pub fn render_template(
    config_dir: &Path,
    template_name: &str,
    ctx: &RenderContext,
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
    fn unknown_template_yields_render_error() {
        let result = render_template(&repo_config_dir(), "does/not/exist.tera", &sample_context());
        assert!(result.is_err());
    }
}
