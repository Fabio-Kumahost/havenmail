//! Havenmail Core — geteilte Bibliothek für Auth, RBAC, Audit-Log und
//! Datenbankzugriff der Control-Plane.
//!
//! Enthält bewusst KEINEN eigenen SMTP-/IMAP-/JMAP-/TLS-Code — diese Crate
//! orchestriert nur (Config-Rendering, DB-Zugriff, Auth-Primitive über
//! etablierte Bibliotheken). Siehe docs/architecture.md im Repo-Root.

pub mod audit;
pub mod auth;
pub mod bootstrap;
pub mod clamav_stats;
pub mod config_render;
pub mod db;
pub mod dkim;
pub mod dns_check;
pub mod fail2ban;
pub mod mail_flow;
pub mod mail_queue;
pub mod mailbox_storage;
pub mod notify;
pub mod rbac;
pub mod rspamd_client;
pub mod service_status;
pub mod tls_status;
pub mod trigger_file;

pub use rbac::{Action, Actor, Role};
